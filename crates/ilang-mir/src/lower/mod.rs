//! AST → MIR lowering.
//!
//! Driven by `lower_program`. Currently covers a working subset of
//! the language; remaining node kinds are listed as `Unsupported`
//! errors so the integration tests fail loudly until we expand
//! coverage. The aim is to grow this file feature-by-feature in the
//! same order as `docs/syntax.md`.

use std::collections::HashMap;

use ilang_ast::{
    self as ast, Block as AstBlock, Expr, ExprKind, FnDecl, Item, Program as AstProgram, Span,
    Stmt, StmtKind, Symbol, Type,
};

mod collect;
mod env;
mod coerce;
mod control;
mod match_;
mod ops;
mod utils;

use collect::{
    collect_cellified_names_block, collect_cellified_names_expr, collect_cellified_names_stmt,
    collect_free_vars_block, collect_mut_assigned_block,
};
use env::{Binding, Env, LoopFrame};
pub use utils::ty_to_mir;
use utils::{cmp_op, mangle_suffix, pick_overload, placeholder_function, retain_if_heap, Cmp};

use crate::builder::FunctionBuilder;
use crate::inst::{
    BinOp, BlockId, FuncId, FuncRef, Inst, MirConst, Terminator, UnOp, ValueId,
};
use crate::program::{FuncParam, Function, FunctionKind, Program};
use crate::types::MirTy;

#[derive(Debug, thiserror::Error)]
pub enum LowerError {
    #[error("{0}")]
    Other(String),
    #[error("unsupported in M1: {0}")]
    Unsupported(&'static str),
}

/// Lower a (post-typecheck) AST `Program` into MIR. The caller is
/// expected to have run the type checker first; we re-derive
/// expression types locally because the AST does not carry them.
pub fn lower_program(prog: &AstProgram) -> Result<Program, LowerError> {
    lower_program_with_slots(prog, &HashMap::new())
}

/// REPL variant: lower with a slot table that maps top-level binding
/// names to a stable host-side slot index plus the binding's AST
/// `Type`. The AST type is converted to `MirTy` lazily inside the
/// lower context so class / enum names can be resolved against the
/// per-program registries.
///
/// At lowering:
///
/// 1. A top-level `let x = expr` whose name is in `slots` gets, after
///    its normal init, a `__repl_store_slot(idx, x_as_i64)` call so
///    the value persists across REPL turns.
/// 2. Any `Var(x)` lookup that misses every local binding (i.e. the
///    name is not bound in this chunk) is resolved by emitting a
///    `__repl_load_slot(idx)` call, then bit-reinterpreting the
///    returned i64 as the slot's declared MirTy.
///
/// All heap-typed slots store an i64 pointer; reinterpretation is a
/// no-op for those. Primitive slots round-trip through coerce.
pub fn lower_program_with_slots(
    prog: &AstProgram,
    slots: &HashMap<Symbol, (u32, ilang_ast::Type)>,
) -> Result<Program, LowerError> {
    let mut lower = Lower::new();
    // Defer slot-type resolution: classes/enums/etc. need the
    // class_ids/enum_ids tables that are populated during the
    // pre-passes below. We resolve and stash them right after
    // `register_class` / `register_enum` complete.
    lower.repl_slot_ast = slots.clone();
    // Built-in `Result<T, E>` is always available — the language
    // reserves the name and the loader doesn't include a stdlib file
    // for it. Pre-register so user `Result.ok(...)` / match on Result
    // resolve.
    lower.register_builtin_result();

    // 1a. Pre-pass: register every class NAME (regular + @extern(C))
    //     before resolving anything. Lets fields reference classes
    //     declared later in the file.
    for item in &prog.items {
        if let Item::Class(cd) = item {
            if !lower.class_ids.contains_key(&cd.name) {
                let id = crate::types::ClassId(lower.classes.len() as u32);
                lower.class_ids.insert(cd.name, id);
                lower.classes.push(crate::program::ClassLayout {
                    id,
                    name: cd.name,
                    parent: None,
                    fields: Vec::new(),
                    methods: Vec::new(),
                    statics: Vec::new(),
                    drop_fn: FuncId(u32::MAX),
                    vtable: None,
                    repr: crate::program::ClassRepr::ArcObject,
                    c_field_offsets: Vec::new(),
                    c_size: 0,
                    flex_elem_size: 0,
                });
                lower.class_meta.insert(id, ClassMeta::default());
            }
        }
    }

    // 1. Register every class shell (id + field indices), enum
    //    layout, and @extern(C) struct shell before any type
    //    resolution or fn declaration so signatures referencing them
    //    by name work.
    for item in &prog.items {
        match item {
            Item::Class(cd) => lower.register_class(cd)?,
            Item::Enum(ed) => lower.register_enum(ed)?,
            Item::ExternC(blk) => lower.register_extern_c_shells(blk)?,
            _ => {}
        }
    }

    // 2. Pre-register every top-level fn (signature only) so calls can
    //    refer to them regardless of declaration order.
    for item in &prog.items {
        if let Item::Fn(fd) = item {
            lower.declare_fn(fd)?;
        }
    }

    // 2b. Pre-register every @extern(C) fn / fn def so class method
    //     bodies that call them resolve correctly.
    for item in &prog.items {
        if let Item::ExternC(blk) = item {
            lower.declare_extern_c_fns(blk)?;
        }
    }

    // 3. Pre-register every method (incl. init) on every class.
    for item in &prog.items {
        if let Item::Class(cd) = item {
            lower.declare_class_methods(cd)?;
        }
    }

    // 3b. REPL slot-type resolution. The class / enum tables are now
    //     populated, so any pending `repl_slot_ast` entry (deferred by
    //     `lower_program_with_slots`) can be promoted to the typed
    //     `repl_slots` map. Entries whose type still doesn't resolve
    //     (e.g. types depending on items not in this chunk) are
    //     silently dropped — the REPL falls back to chunk-local
    //     binding semantics for those names.
    let pending_slots = std::mem::take(&mut lower.repl_slot_ast);
    for (name, (idx, ty)) in pending_slots {
        if let Ok(mir_ty) = lower.resolve_ty(&ty) {
            lower.repl_slots.insert(name, (idx, mir_ty));
        }
    }

    // 4. Lower bodies: free fns, then class methods.
    for item in &prog.items {
        match item {
            Item::Fn(fd) => lower.lower_fn(fd)?,
            Item::Class(cd) => lower.lower_class_methods(cd)?,
            // Enums have no bodies — registration handled them.
            Item::Enum(_) => {}
            Item::Const(_) => {
                // Loader's inline pass folds constants away before
                // type checking — we shouldn't see this here.
                return Err(LowerError::Other("unexpected Item::Const after loader inlining".into()));
            }
            Item::Use(_) => {
                return Err(LowerError::Other("unexpected Item::Use post-loader".into()));
            }
            Item::ExternC(blk) => lower.lower_extern_c(blk)?,
        }
    }

    // Synthesise __main from the program's top-level statements + tail.
    lower.lower_main(&prog.stmts, prog.tail.as_ref())?;

    // Drain any pending closure bodies (closures-of-closures included).
    while let Some(pc) = lower.pending_closures.pop() {
        lower.lower_pending_closure(pc)?;
    }

    Ok(lower.finish())
}

// ---------------------------------------------------------------- //

struct Lower {
    /// All MIR functions accumulated so far. The entry point goes in
    /// last (after main is built).
    funcs: Vec<Function>,
    /// Top-level fn name → FuncId, populated by the pre-registration
    /// pass. Indexes into `funcs` once `lower_fn` has been called.
    fn_ids: HashMap<Symbol, FuncId>,
    /// Top-level fn signatures (param/ret types, MIR side). Used by
    /// callers to know the return type without re-checking.
    fn_sigs: HashMap<Symbol, FnSig>,
    /// Registered class layouts (one per class declaration).
    classes: Vec<crate::program::ClassLayout>,
    /// Class name → ClassId.
    class_ids: HashMap<Symbol, crate::types::ClassId>,
    /// Per-class metadata used during lowering: field name → FieldId,
    /// method name → FuncId for the (mangle-resolved) implementation.
    class_meta: HashMap<crate::types::ClassId, ClassMeta>,
    /// Registered enum layouts.
    enums: Vec<crate::program::EnumLayout>,
    enum_ids: HashMap<Symbol, crate::types::EnumId>,
    enum_meta: HashMap<crate::types::EnumId, EnumMeta>,
    /// Top-level static slots accumulated across classes.
    statics: Vec<crate::program::StaticSlot>,
    /// Synthesised closure functions waiting for their bodies to be
    /// lowered. Drained after each outer fn body completes so that
    /// nested closures get processed without recursive borrows.
    pending_closures: Vec<PendingClosure>,
    /// Counter for anonymous fn names.
    anon_counter: u32,
    /// Per @extern(C) @lib fn binding metadata.
    extern_meta: HashMap<Symbol, ExternMeta>,
    /// `__main`'s FuncId, set during `lower_main`. Subsequent pending
    /// closures push more functions, so `funcs.len() - 1` no longer
    /// identifies the entry.
    main_id: Option<FuncId>,
    /// User-visible name → list of mangled names (each registered in
    /// `fn_ids` / `fn_sigs`). Most entries have a single mangled name
    /// equal to the user name; overloaded fns have one entry per
    /// declared overload.
    overloads: HashMap<Symbol, Vec<Symbol>>,
    /// REPL persistent slots: name → (slot index, MirTy). Set via
    /// [`lower_program_with_slots`]. Empty for non-REPL compilations.
    /// Drives host-slot store/load emission in __main and Var lookup.
    /// Resolved lazily from `repl_slot_ast` after the class / enum
    /// pre-passes register their ids.
    repl_slots: HashMap<Symbol, (u32, MirTy)>,
    /// Pre-resolution slot table from the REPL caller. Each entry is
    /// converted to `repl_slots[name]` once the lowerer's class /
    /// enum tables are populated.
    repl_slot_ast: HashMap<Symbol, (u32, ilang_ast::Type)>,
}

// Recorded for every `@extern(C) @lib(..)` fn while lowering. The
// MIR-codegen reads these fields off `Function` directly today, so
// the fields here are bookkeeping for any future passes that might
// want richer per-extern metadata in the AST→MIR layer.
#[allow(dead_code)]
struct ExternMeta {
    libs: Vec<Symbol>,
    optional: bool,
    variadic: bool,
    c_symbol: Symbol,
}

struct PendingClosure {
    func_id: FuncId,
    name: Symbol,
    params: Vec<(Symbol, MirTy)>,
    ret: MirTy,
    captures: Vec<crate::program::EnvCapture>,
    body: ast::Block,
    span: Span,
    /// Outer-method class context — preserved so `super.method(...)`
    /// inside the closure body can resolve to the parent class.
    enclosing_this_class: Option<crate::types::ClassId>,
}

#[derive(Default)]
struct EnumMeta {
    /// Variant name → (VariantId, discriminant, payload kind).
    variants: HashMap<Symbol, EnumVariantMeta>,
}

struct EnumVariantMeta {
    id: crate::inst::VariantId,
    payload: VariantPayloadMeta,
}

#[derive(Clone)]
enum VariantPayloadMeta {
    Unit,
    /// Tuple variant — element MirTys in order.
    Tuple(Vec<MirTy>),
    /// Struct variant — field name → (idx, MirTy).
    Struct(Vec<(Symbol, MirTy)>),
}

#[derive(Default)]
struct ClassMeta {
    field_ix: HashMap<Symbol, crate::inst::FieldId>,
    field_ty: HashMap<crate::inst::FieldId, MirTy>,
    /// Includes both regular methods and `init` (under the symbol "init").
    method_ids: HashMap<Symbol, FuncId>,
    method_sigs: HashMap<Symbol, FnSig>,
    /// `static name(...): T { ... }` — called as `Class.method(...)`.
    static_method_ids: HashMap<Symbol, FuncId>,
    static_method_sigs: HashMap<Symbol, FnSig>,
    /// `static name: T = ...` / `const name: T = ...` slots.
    static_slots: HashMap<Symbol, crate::inst::StaticSlotId>,
    /// `get name(): T` — synthesised method id for the getter.
    property_getter: HashMap<Symbol, (FuncId, MirTy)>,
    /// `set name(v: T)` — synthesised method id for the setter.
    property_setter: HashMap<Symbol, (FuncId, MirTy)>,
}

#[derive(Clone)]
pub(super) struct FnSig {
    pub(super) params: Vec<MirTy>,
    pub(super) ret: MirTy,
}

impl Lower {
    fn register_builtin_result(&mut self) {
        // Built-in `Result<T, E>`. Treated as non-generic at MIR level
        // (T and E both flow as i64 cells); the codegen reads payload
        // bytes uniformly.
        let id = crate::types::EnumId(self.enums.len() as u32);
        let name = Symbol::intern("Result");
        self.enum_ids.insert(name, id);
        let mut meta = EnumMeta::default();
        let ok_id = crate::inst::VariantId(0);
        let err_id = crate::inst::VariantId(1);
        meta.variants.insert(
            Symbol::intern("ok"),
            EnumVariantMeta {
                id: ok_id,
                payload: VariantPayloadMeta::Tuple(vec![MirTy::I64]),
            },
        );
        meta.variants.insert(
            Symbol::intern("err"),
            EnumVariantMeta {
                id: err_id,
                payload: VariantPayloadMeta::Tuple(vec![MirTy::I64]),
            },
        );
        self.enums.push(crate::program::EnumLayout {
            id,
            name,
            repr: MirTy::I64,
            variants: vec![
                crate::program::VariantDecl {
                    id: ok_id,
                    name: Symbol::intern("ok"),
                    discriminant: 0,
                    discriminant_str: None,
                    payload: crate::program::VariantPayload::Tuple(
                        vec![MirTy::I64].into_boxed_slice(),
                    ),
                },
                crate::program::VariantDecl {
                    id: err_id,
                    name: Symbol::intern("err"),
                    discriminant: 1,
                    discriminant_str: None,
                    payload: crate::program::VariantPayload::Tuple(
                        vec![MirTy::I64].into_boxed_slice(),
                    ),
                },
            ],
            is_flags: false,
        });
        self.enum_meta.insert(id, meta);
    }

    fn new() -> Self {
        Self {
            funcs: Vec::new(),
            fn_ids: HashMap::new(),
            fn_sigs: HashMap::new(),
            classes: Vec::new(),
            class_ids: HashMap::new(),
            class_meta: HashMap::new(),
            enums: Vec::new(),
            enum_ids: HashMap::new(),
            enum_meta: HashMap::new(),
            statics: Vec::new(),
            pending_closures: Vec::new(),
            anon_counter: 0,
            extern_meta: HashMap::new(),
            main_id: None,
            overloads: HashMap::new(),
            repl_slots: HashMap::new(),
            repl_slot_ast: HashMap::new(),
        }
    }

    fn finish(mut self) -> Program {
        // `lower_main` records the entry id; subsequent pending
        // closures push more functions afterwards, so we can't rely
        // on `funcs.len() - 1`.
        let entry = self
            .main_id
            .unwrap_or_else(|| FuncId((self.funcs.len() - 1) as u32));
        let mut p = Program::new(entry);
        p.functions = std::mem::take(&mut self.funcs);
        p.classes = std::mem::take(&mut self.classes);
        p.enums = std::mem::take(&mut self.enums);
        p.statics = std::mem::take(&mut self.statics);
        p
    }

    fn resolve_ty(&self, t: &Type) -> Result<MirTy, LowerError> {
        match t {
            // The parser doesn't know whether a bare `Foo` names a class
            // or an enum — both surface as `Type::Object`. Resolve here
            // by checking the registry that holds the name.
            Type::Object(name) => {
                if let Some(id) = self.class_ids.get(name) {
                    Ok(MirTy::Object(*id))
                } else if let Some(id) = self.enum_ids.get(name) {
                    Ok(MirTy::Enum(*id))
                } else {
                    Err(LowerError::Other(format!("unknown type: {name}")))
                }
            }
            Type::Enum(name) => match self.enum_ids.get(name) {
                Some(id) => Ok(MirTy::Enum(*id)),
                None => Err(LowerError::Other(format!("unknown enum type: {name}"))),
            },
            Type::Array { elem, fixed } => Ok(MirTy::Array {
                elem: Box::new(self.resolve_ty(elem)?),
                len: *fixed,
            }),
            Type::Tuple(elems) => {
                let mut out = Vec::with_capacity(elems.len());
                for e in elems.iter() {
                    out.push(self.resolve_ty(e)?);
                }
                Ok(MirTy::Tuple(out.into_boxed_slice()))
            }
            Type::Optional(inner) => Ok(MirTy::Optional(Box::new(self.resolve_ty(inner)?))),
            Type::Weak(inner) => match &**inner {
                Type::Object(cname) => match self.class_ids.get(cname) {
                    Some(id) => Ok(MirTy::Weak(*id)),
                    None => Err(LowerError::Other(format!("unknown class for weak ref: {cname}"))),
                },
                _ => Err(LowerError::Other("`.weak` only applies to class types".into())),
            },
            Type::Generic(g) => {
                // Built-in `Map<K, V>` is special-cased here.
                if g.base.as_str() == "Map" && g.args.len() == 2 {
                    let key = self.resolve_ty(&g.args[0])?;
                    let val = self.resolve_ty(&g.args[1])?;
                    return Ok(MirTy::Map { key: Box::new(key), val: Box::new(val) });
                }
                // Built-in `Result<T, E>` is registered as a
                // non-generic enum (i64 payload cells) — fall through
                // by name lookup.
                if let Some(id) = self.enum_ids.get(&g.base) {
                    return Ok(MirTy::Enum(*id));
                }
                Err(LowerError::Unsupported("user-defined generic types"))
            }
            Type::Fn(ft) => {
                let mut params = Vec::with_capacity(ft.params.len());
                for p in ft.params.iter() {
                    params.push(self.resolve_ty(p)?);
                }
                let ret = self.resolve_ty(&ft.ret)?;
                Ok(MirTy::Fn(Box::new(crate::types::MirFnTy {
                    params: params.into_boxed_slice(),
                    ret,
                })))
            }
            Type::RawPtr { is_const, inner } => Ok(MirTy::RawPtr {
                is_const: *is_const,
                inner: Box::new(self.resolve_ty(inner)?),
            }),
            other => ty_to_mir(other),
        }
    }

    fn register_extern_c_shells(&mut self, blk: &ast::ExternCBlock) -> Result<(), LowerError> {
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
    fn c_size_align_of(&self, t: &MirTy) -> (i64, i64) {
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
    fn validate_extern_c_by_value(&self, params: &[MirTy]) -> Result<(), LowerError> {
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
    fn declare_extern_c_fns(&mut self, blk: &ast::ExternCBlock) -> Result<(), LowerError> {
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

    fn lower_extern_c(&mut self, blk: &ast::ExternCBlock) -> Result<(), LowerError> {
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

    fn register_enum(&mut self, ed: &ast::EnumDecl) -> Result<(), LowerError> {
        if !ed.type_params.is_empty() {
            return Err(LowerError::Unsupported("generic enums"));
        }
        let id = crate::types::EnumId(self.enums.len() as u32);
        self.enum_ids.insert(ed.name, id);

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
        self.enums.push(crate::program::EnumLayout {
            id,
            name: ed.name,
            repr: repr_ty,
            variants,
            is_flags: ed.flags,
        });
        self.enum_meta.insert(id, meta);
        Ok(())
    }

    fn declare_fn(&mut self, fd: &FnDecl) -> Result<(), LowerError> {
        if !fd.type_params.is_empty() {
            return Err(LowerError::Unsupported("generic functions"));
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

    /// Allocate a `ClassId` and field-index table for a class
    /// declaration. Method registration is deferred until
    /// `declare_class_methods` so that a class's own fields can be
    /// resolved by its method signatures.
    fn register_class(&mut self, cd: &ast::ClassDecl) -> Result<(), LowerError> {
        if !cd.type_params.is_empty() {
            return Err(LowerError::Unsupported("generic classes"));
        }
        if cd.is_repr_c || cd.is_packed || cd.is_union || cd.extern_lib.is_some() {
            return Err(LowerError::Unsupported("@extern(C) classes"));
        }
        // Static methods, fields, const, and properties are wired
        // below in declare_class_methods / register_class.

        let parent_id = if let Some(parent_name) = cd.parent {
            Some(*self.class_ids.get(&parent_name).ok_or_else(|| {
                LowerError::Other(format!("parent class {parent_name} not declared yet"))
            })?)
        } else {
            None
        };

        // The pre-pass in `lower_program` may have already reserved
        // an id + placeholder layout for this class to enable forward
        // references. Reuse it when present.
        let id = match self.class_ids.get(&cd.name) {
            Some(existing) => *existing,
            None => {
                let id = crate::types::ClassId(self.classes.len() as u32);
                self.class_ids.insert(cd.name, id);
                self.classes.push(crate::program::ClassLayout {
                    id,
                    name: cd.name,
                    parent: None,
                    fields: Vec::new(),
                    methods: Vec::new(),
                    statics: Vec::new(),
                    drop_fn: FuncId(u32::MAX),
                    vtable: None,
                    repr: crate::program::ClassRepr::ArcObject,
                    c_field_offsets: Vec::new(),
                    c_size: 0,
                    flex_elem_size: 0,
                });
                id
            }
        };
        if !self.class_meta.contains_key(&id) {
            self.class_meta.insert(id, ClassMeta::default());
        }

        let mut meta = ClassMeta::default();
        let mut fields = Vec::new();
        let mut next_fid: u32 = 0;
        // Inherit parent fields first (preserve their FieldIds as
        // contiguous indexes into the child's field vec).
        if let Some(pid) = parent_id {
            let parent = &self.classes[pid.0 as usize].clone();
            for f in &parent.fields {
                let fid = crate::inst::FieldId(next_fid);
                next_fid += 1;
                meta.field_ix.insert(f.name, fid);
                meta.field_ty.insert(fid, f.ty.clone());
                fields.push(crate::program::FieldDecl {
                    id: fid,
                    name: f.name,
                    ty: f.ty.clone(),
                    bit_field: None,
                });
            }
        }
        for fd in cd.fields.iter() {
            let fid = crate::inst::FieldId(next_fid);
            next_fid += 1;
            let fty = self.resolve_ty(&fd.ty)?;
            meta.field_ix.insert(fd.name, fid);
            meta.field_ty.insert(fid, fty.clone());
            fields.push(crate::program::FieldDecl {
                id: fid,
                name: fd.name,
                ty: fty,
                bit_field: None,
            });
        }
        // Static / const fields → StaticSlot table.
        let mut static_slot_ids = Vec::new();
        for sf in cd.static_fields.iter() {
            let slot_id = crate::inst::StaticSlotId(self.statics.len() as u32);
            let ty = self.resolve_ty(&sf.ty)?;
            let init_const = match &sf.value.kind {
                ExprKind::Int(n) => MirConst::Int(*n),
                ExprKind::Float(f) => MirConst::F64(f.to_bits()),
                ExprKind::Bool(b) => MirConst::Bool(*b),
                ExprKind::Str(s) => MirConst::Str(Symbol::intern(s)),
                // Non-literal initializer — the loader has
                // emitted a synthetic top-level
                // `ClassName.field = expr` (with `is_init:
                // true`) so the slot gets the real value at
                // program startup. Use a typed zero default
                // here just for the slot's static layout.
                _ => match &ty {
                    MirTy::F32 | MirTy::F64 => MirConst::F64(0u64),
                    MirTy::Bool => MirConst::Bool(false),
                    MirTy::Str => MirConst::Str(Symbol::intern("")),
                    _ => MirConst::Int(0),
                },
            };
            self.statics.push(crate::program::StaticSlot {
                id: slot_id,
                owner: id,
                name: sf.name,
                ty,
                is_const: sf.is_const,
                init: init_const,
            });
            static_slot_ids.push(slot_id);
            meta.static_slots.insert(sf.name, slot_id);
        }

        // Update the placeholder layout in place — the pre-pass
        // already pushed it onto `self.classes`.
        let layout = &mut self.classes[id.0 as usize];
        layout.parent = parent_id;
        layout.fields = fields;
        layout.statics = static_slot_ids;
        layout.repr = crate::program::ClassRepr::ArcObject;
        self.class_meta.insert(id, meta);
        Ok(())
    }

    /// Pre-register signatures for every method on the class so that
    /// in-class calls (`this.foo()` / cross-method) resolve regardless
    /// of declaration order.
    fn declare_class_methods(&mut self, cd: &ast::ClassDecl) -> Result<(), LowerError> {
        let class_id = *self.class_ids.get(&cd.name).expect("class registered");
        let class_obj_ty = MirTy::Object(class_id);
        let mut method_decls = Vec::new();

        // Inherit parent's method registry (init / deinit are
        // per-class; instance methods carry over for `super` resolution
        // and direct calls). Override below replaces the FuncId.
        let parent_id = self.classes[class_id.0 as usize].parent;
        if let Some(pid) = parent_id {
            let parent_meta_clone: Vec<(Symbol, FuncId, FnSig)> = self
                .class_meta
                .get(&pid)
                .map(|m| {
                    m.method_ids
                        .iter()
                        .filter(|(n, _)| n.as_str() != "init" && n.as_str() != "deinit")
                        .map(|(name, fid)| {
                            let sig = m.method_sigs.get(name).cloned().unwrap();
                            (*name, *fid, sig)
                        })
                        .collect()
                })
                .unwrap_or_default();
            // Re-sign parent methods so the receiver type points to
            // this class instead of the parent (subtype substitution).
            let meta = self.class_meta.get_mut(&class_id).unwrap();
            for (name, fid, sig) in parent_meta_clone {
                let mut new_sig = sig.clone();
                if let Some(first) = new_sig.params.first_mut() {
                    *first = class_obj_ty.clone();
                }
                meta.method_ids.insert(name, fid);
                meta.method_sigs.insert(name, new_sig);
                method_decls.push(crate::program::MethodDecl {
                    name,
                    is_override: false,
                    is_static: false,
                    func: fid,
                    slot: None,
                });
            }
        }

        for m in cd.methods.iter() {
            if !m.type_params.is_empty() {
                return Err(LowerError::Unsupported("generic methods"));
            }
            // Mangled name: `Class.method` (init included). This is the
            // post-overload-resolution name; until overloading is wired,
            // we use a single function per (class, method) pair.
            let mangled = Symbol::intern(&format!("{}.{}", cd.name, m.name));
            let id = FuncId(self.funcs.len() as u32);
            self.funcs.push(placeholder_function(mangled));
            self.fn_ids.insert(mangled, id);

            // Method signature: `this` is an implicit first parameter.
            let mut params = vec![class_obj_ty.clone()];
            for p in m.params.iter() {
                params.push(self.resolve_ty(&p.ty)?);
            }
            // `init` synthesises a return of `this` so callers of
            // `new C(args)` get the constructed instance back.
            let user_ret = match &m.ret {
                Some(t) => self.resolve_ty(t)?,
                None => MirTy::Unit,
            };
            let ret = if m.name == "init" {
                MirTy::Object(class_id)
            } else {
                user_ret
            };
            self.fn_sigs.insert(mangled, FnSig { params: params.clone(), ret: ret.clone() });

            let kind = if m.name == "init" {
                FunctionKind::Init { class: class_id }
            } else if m.name == "deinit" {
                // Record the deinit fn as the class's drop fn so
                // codegen can call it on Release.
                self.classes[class_id.0 as usize].drop_fn = id;
                FunctionKind::Drop { class: class_id }
            } else {
                FunctionKind::Local
            };
            // Patch the placeholder with the right kind so
            // post-lowering consumers can recognise it.
            self.funcs[id.0 as usize].kind = kind.clone();
            self.funcs[id.0 as usize].name = mangled;

            let meta = self.class_meta.get_mut(&class_id).unwrap();
            meta.method_ids.insert(m.name, id);
            meta.method_sigs.insert(m.name, FnSig { params, ret });
            // Replace any inherited entry of the same name (override).
            method_decls.retain(|d: &crate::program::MethodDecl| d.name != m.name);
            method_decls.push(crate::program::MethodDecl {
                name: m.name,
                is_override: m.is_override,
                is_static: false,
                func: id,
                slot: None,
            });
        }

        // If this class doesn't define its own deinit but inherits
        // from a parent that has one, point our drop_fn at the
        // parent's so dropping a subclass instance still triggers
        // the parent's destruction chain. Parent classes are
        // processed before children (source-order requirement), so
        // the parent's drop_fn is already set by the time we get
        // here.
        if self.classes[class_id.0 as usize].drop_fn == FuncId(u32::MAX) {
            if let Some(pid) = parent_id {
                let parent_drop = self.classes[pid.0 as usize].drop_fn;
                if parent_drop != FuncId(u32::MAX) {
                    self.classes[class_id.0 as usize].drop_fn = parent_drop;
                }
            }
        }

        // Static methods — registered as top-level fns under
        // `Class.method`.
        for sm in cd.static_methods.iter() {
            if !sm.type_params.is_empty() {
                return Err(LowerError::Unsupported("generic static methods"));
            }
            let mangled = Symbol::intern(&format!("{}.{}", cd.name, sm.name));
            let id = FuncId(self.funcs.len() as u32);
            self.funcs.push(placeholder_function(mangled));
            self.fn_ids.insert(mangled, id);

            let params: Vec<MirTy> = sm
                .params
                .iter()
                .map(|p| self.resolve_ty(&p.ty))
                .collect::<Result<Vec<_>, _>>()?;
            let ret = match &sm.ret {
                Some(t) => self.resolve_ty(t)?,
                None => MirTy::Unit,
            };
            self.fn_sigs.insert(
                mangled,
                FnSig { params: params.clone(), ret: ret.clone() },
            );
            self.funcs[id.0 as usize].name = mangled;
            self.funcs[id.0 as usize].kind = FunctionKind::Local;

            let meta = self.class_meta.get_mut(&class_id).unwrap();
            meta.static_method_ids.insert(sm.name, id);
            meta.static_method_sigs.insert(sm.name, FnSig { params, ret });
        }

        // Properties — synthesise getter/setter as methods.
        for prop in cd.properties.iter() {
            let prop_ty = self.resolve_ty(&prop.ty)?;
            let class_obj_ty = MirTy::Object(class_id);
            if let Some(_getter_decl) = &prop.getter {
                let mangled = Symbol::intern(&format!("{}.get_{}", cd.name, prop.name));
                let id = FuncId(self.funcs.len() as u32);
                self.funcs.push(placeholder_function(mangled));
                self.fn_ids.insert(mangled, id);
                let params = vec![class_obj_ty.clone()];
                let ret = prop_ty.clone();
                self.fn_sigs.insert(
                    mangled,
                    FnSig { params: params.clone(), ret: ret.clone() },
                );
                self.funcs[id.0 as usize].name = mangled;
                self.funcs[id.0 as usize].kind = FunctionKind::Local;
                // Synthesise unique keys for property getter/setter so
                // they don't collide with each other or with regular
                // methods of the same name.
                let key = Symbol::intern(&format!("{}::get", prop.name));
                let meta = self.class_meta.get_mut(&class_id).unwrap();
                meta.property_getter.insert(prop.name, (id, prop_ty.clone()));
                meta.method_sigs.insert(key, FnSig { params, ret });
                meta.method_ids.insert(key, id);
            }
            if let Some(_setter_decl) = &prop.setter {
                let mangled = Symbol::intern(&format!("{}.set_{}", cd.name, prop.name));
                let id = FuncId(self.funcs.len() as u32);
                self.funcs.push(placeholder_function(mangled));
                self.fn_ids.insert(mangled, id);
                let params = vec![class_obj_ty.clone(), prop_ty.clone()];
                let ret = MirTy::Unit;
                self.fn_sigs.insert(
                    mangled,
                    FnSig { params: params.clone(), ret: ret.clone() },
                );
                self.funcs[id.0 as usize].name = mangled;
                self.funcs[id.0 as usize].kind = FunctionKind::Local;
                let key = Symbol::intern(&format!("{}::set", prop.name));
                let meta = self.class_meta.get_mut(&class_id).unwrap();
                meta.property_setter.insert(prop.name, (id, prop_ty.clone()));
                meta.method_sigs.insert(key, FnSig { params, ret });
                meta.method_ids.insert(key, id);
            }
        }

        // Assign vtable slots: inherit parent slots for same-named
        // methods, append new slots otherwise. Init / deinit aren't
        // dispatched virtually.
        let parent_slots: HashMap<Symbol, crate::inst::VTableSlot> = match parent_id {
            Some(pid) => self.classes[pid.0 as usize]
                .methods
                .iter()
                .filter_map(|m| m.slot.map(|s| (m.name, s)))
                .collect(),
            None => HashMap::new(),
        };
        let mut next_slot: u32 = parent_slots
            .values()
            .map(|s| s.0 + 1)
            .max()
            .unwrap_or(0);
        for d in method_decls.iter_mut() {
            if matches!(d.name.as_str(), "init" | "deinit") {
                continue;
            }
            let slot = match parent_slots.get(&d.name) {
                Some(s) => *s,
                None => {
                    let s = crate::inst::VTableSlot(next_slot);
                    next_slot += 1;
                    s
                }
            };
            d.slot = Some(slot);
        }
        let layout = &mut self.classes[class_id.0 as usize];
        layout.methods = method_decls;
        Ok(())
    }

    fn lower_class_methods(&mut self, cd: &ast::ClassDecl) -> Result<(), LowerError> {
        let class_id = *self.class_ids.get(&cd.name).expect("class registered");
        for m in cd.methods.iter() {
            self.lower_method(class_id, cd.name, m)?;
        }
        // Static methods → lower like a free fn (no `this` injection).
        for sm in cd.static_methods.iter() {
            self.lower_static_method(class_id, cd.name, sm)?;
        }
        // Property getters/setters — lowered like instance methods,
        // but the m.name passed in for the lookup is a synthetic
        // `prop::get` / `prop::set` key (built in declare_class_methods).
        for prop in cd.properties.iter() {
            if let Some(g) = &prop.getter {
                let mut g2 = g.clone();
                g2.name = Symbol::intern(&format!("{}::get", prop.name));
                self.lower_method(class_id, cd.name, &g2)?;
            }
            if let Some(s) = &prop.setter {
                let mut s2 = s.clone();
                s2.name = Symbol::intern(&format!("{}::set", prop.name));
                self.lower_method(class_id, cd.name, &s2)?;
            }
        }
        Ok(())
    }

    fn lower_pending_closure(&mut self, pc: PendingClosure) -> Result<(), LowerError> {
        let mut fb = FunctionBuilder::new(pc.name, pc.name, pc.ret.clone(), FunctionKind::Local);
        fb.set_span(pc.span);
        let entry = fb.new_block();
        fb.switch_to(entry);

        let mut env = Env::default();
        let mut params_box = Vec::with_capacity(pc.params.len());
        for (pname, pty) in pc.params.iter() {
            let v = fb.add_block_param(entry, pty.clone());
            env.bind(*pname, v, pty.clone());
            params_box.push(FuncParam {
                name: *pname,
                ty: pty.clone(),
                value: v,
            });
        }

        // Build the captures-in-scope map.
        let mut caps_map: HashMap<Symbol, (u32, MirTy)> = HashMap::new();
        let mut cell_caps_set: std::collections::HashSet<Symbol> =
            std::collections::HashSet::new();
        for (i, c) in pc.captures.iter().enumerate() {
            caps_map.insert(c.name, (i as u32, c.ty.clone()));
            if c.is_cell {
                cell_caps_set.insert(c.name);
            }
        }

        // Names cellified inside this closure body too (for nested
        // FnExprs that mutate further captures).
        let mut cellify_inner: std::collections::HashSet<Symbol> =
            std::collections::HashSet::new();
        collect_cellified_names_block(&pc.body, &mut cellify_inner);

        let __cells_empty: std::collections::HashSet<Symbol> = std::collections::HashSet::new();
        let mut bcx = BodyCx {
            fb: &mut fb,
            env: &mut env,
            ret_ty: pc.ret.clone(),
            fn_ids: &mut self.fn_ids,
            fn_sigs: &mut self.fn_sigs,
            loops: Vec::new(),
            this_class: pc.enclosing_this_class,
            classes: &self.classes,
            class_meta: &self.class_meta,
            enum_ids: &self.enum_ids,
            enum_meta: &self.enum_meta,
            enums: &self.enums,
            statics: &self.statics,
            pending: &mut self.pending_closures,
            funcs: &mut self.funcs,
            anon_counter: &mut self.anon_counter,
            captures_in_scope: Some(&caps_map),
            cell_captures: Some(&cell_caps_set),
            overloads: &self.overloads,
            cellify_set: &cellify_inner,
            repl_slots: &self.repl_slots,
            is_main_body: false,
            binding_self_name: None,
            crepr_owned_locals: std::collections::HashSet::new(),
        };
        let needs_retain = pc
            .body
            .tail
            .as_ref()
            .map(|e| bcx.callee_retain_decision(e))
            .unwrap_or(false);
        let tail = bcx.lower_block(&pc.body)?;
        if needs_retain {
            bcx.emit_callee_retain(&tail);
        }
        bcx.finalise_return(tail)?;

        let env_layout = if pc.captures.is_empty() {
            None
        } else {
            Some(crate::program::EnvLayout {
                captures: pc.captures.clone(),
            })
        };
        let mut func = fb.finish(params_box.into_boxed_slice());
        func.closure_env = env_layout;
        self.funcs[pc.func_id.0 as usize] = func;
        Ok(())
    }

    fn lower_static_method(
        &mut self,
        class_id: crate::types::ClassId,
        class_name: Symbol,
        m: &FnDecl,
    ) -> Result<(), LowerError> {
        let id = *self
            .class_meta
            .get(&class_id)
            .unwrap()
            .static_method_ids
            .get(&m.name)
            .expect("static method pre-registered");
        let sig = self
            .class_meta
            .get(&class_id)
            .unwrap()
            .static_method_sigs
            .get(&m.name)
            .cloned()
            .expect("static sig pre-registered");

        let mangled = Symbol::intern(&format!("{}.{}", class_name, m.name));
        let mut fb = FunctionBuilder::new(mangled, m.name, sig.ret.clone(), FunctionKind::Local);
        fb.set_span(m.span);
        let entry = fb.new_block();
        fb.switch_to(entry);

        let mut env = Env::default();
        let mut params_box = Vec::with_capacity(sig.params.len());
        for (param, pty) in m.params.iter().zip(sig.params.iter()) {
            let v = fb.add_block_param(entry, pty.clone());
            env.bind(param.name, v, pty.clone());
            params_box.push(FuncParam {
                name: param.name,
                ty: pty.clone(),
                value: v,
            });
        }

        let mut __cellify: std::collections::HashSet<Symbol> = std::collections::HashSet::new();
        collect_cellified_names_block(&m.body, &mut __cellify);
        let mut bcx = BodyCx {
            fb: &mut fb,
            env: &mut env,
            ret_ty: sig.ret.clone(),
            fn_ids: &mut self.fn_ids,
            fn_sigs: &mut self.fn_sigs,
            loops: Vec::new(),
            this_class: None, // static — no `this`
            classes: &self.classes,
            class_meta: &self.class_meta,
            enum_ids: &self.enum_ids,
            enum_meta: &self.enum_meta,
            enums: &self.enums,
            statics: &self.statics,
            pending: &mut self.pending_closures,
            funcs: &mut self.funcs,
            anon_counter: &mut self.anon_counter,
            captures_in_scope: None,
            cell_captures: None,
            cellify_set: &__cellify,
            overloads: &self.overloads,
            repl_slots: &self.repl_slots,
            is_main_body: false,
            binding_self_name: None,
            crepr_owned_locals: std::collections::HashSet::new(),
        };
        let needs_retain = m
            .body
            .tail
            .as_ref()
            .map(|e| bcx.callee_retain_decision(e))
            .unwrap_or(false);
        let tail = bcx.lower_block(&m.body)?;
        if needs_retain {
            bcx.emit_callee_retain(&tail);
        }
        bcx.finalise_return(tail)?;

        let func = fb.finish(params_box.into_boxed_slice());
        self.funcs[id.0 as usize] = func;
        Ok(())
    }

    fn lower_method(
        &mut self,
        class_id: crate::types::ClassId,
        _class_name: Symbol,
        m: &FnDecl,
    ) -> Result<(), LowerError> {
        let id = *self
            .class_meta
            .get(&class_id)
            .unwrap()
            .method_ids
            .get(&m.name)
            .expect("method pre-registered");
        let sig = self
            .class_meta
            .get(&class_id)
            .unwrap()
            .method_sigs
            .get(&m.name)
            .cloned()
            .expect("method sig pre-registered");

        // Use the FuncId's pre-registered name so property getters /
        // setters keep their unique `Class.get_<prop>` identity (the
        // `m.name` we got is the synthetic `<prop>::get` key).
        let mangled = self.funcs[id.0 as usize].name;
        let kind = self.funcs[id.0 as usize].kind.clone();
        let mut fb = FunctionBuilder::new(mangled, m.name, sig.ret.clone(), kind);
        fb.set_span(m.span);
        let entry = fb.new_block();
        fb.switch_to(entry);

        // First param is `this`.
        let this_v = fb.add_block_param(entry, sig.params[0].clone());
        let mut params_box = vec![FuncParam {
            name: Symbol::intern("this"),
            ty: sig.params[0].clone(),
            value: this_v,
        }];

        let mut env = Env::default();
        env.bind(Symbol::intern("this"), this_v, sig.params[0].clone());

        for (param, pty) in m.params.iter().zip(sig.params.iter().skip(1)) {
            let v = fb.add_block_param(entry, pty.clone());
            env.bind(param.name, v, pty.clone());
            params_box.push(FuncParam {
                name: param.name,
                ty: pty.clone(),
                value: v,
            });
        }

        let mut __cellify: std::collections::HashSet<Symbol> = std::collections::HashSet::new();
        collect_cellified_names_block(&m.body, &mut __cellify);
        let mut bcx = BodyCx {
            fb: &mut fb,
            env: &mut env,
            ret_ty: sig.ret.clone(),
            fn_ids: &mut self.fn_ids,
            fn_sigs: &mut self.fn_sigs,
            loops: Vec::new(),
            this_class: Some(class_id),
            classes: &self.classes,
            class_meta: &self.class_meta,
            enum_ids: &self.enum_ids,
            enum_meta: &self.enum_meta,
            enums: &self.enums,
            statics: &self.statics,
            pending: &mut self.pending_closures,
            funcs: &mut self.funcs,
            anon_counter: &mut self.anon_counter,
            captures_in_scope: None,
            cell_captures: None,
            cellify_set: &__cellify,
            overloads: &self.overloads,
            repl_slots: &self.repl_slots,
            is_main_body: false,
            binding_self_name: None,
            crepr_owned_locals: std::collections::HashSet::new(),
        };
        let needs_retain = m
            .body
            .tail
            .as_ref()
            .map(|e| bcx.callee_retain_decision(e))
            .unwrap_or(false);
        let tail = bcx.lower_block(&m.body)?;
        let is_init = matches!(m.name.as_str(), "init");
        if is_init {
            bcx.fb.set_terminator(Terminator::Return { value: Some(this_v) });
        } else {
            if needs_retain {
                bcx.emit_callee_retain(&tail);
            }
            bcx.finalise_return(tail)?;
        }

        let func = fb.finish(params_box.into_boxed_slice());
        self.funcs[id.0 as usize] = func;
        Ok(())
    }

    fn lower_fn(&mut self, fd: &FnDecl) -> Result<(), LowerError> {
        // Resolve the right mangled name by matching this FnDecl's
        // param types against the candidates registered for `fd.name`.
        let target_params: Vec<MirTy> = fd
            .params
            .iter()
            .map(|p| self.resolve_ty(&p.ty))
            .collect::<Result<Vec<_>, _>>()?;
        let candidates = self
            .overloads
            .get(&fd.name)
            .cloned()
            .unwrap_or_default();
        let mangled = candidates
            .iter()
            .copied()
            .find(|m| {
                self.fn_sigs
                    .get(m)
                    .map(|s| s.params == target_params)
                    .unwrap_or(false)
            })
            .unwrap_or(fd.name);
        let sig = self
            .fn_sigs
            .get(&mangled)
            .cloned()
            .ok_or_else(|| LowerError::Other(format!("fn {} not pre-registered", fd.name)))?;
        let id = *self.fn_ids.get(&mangled).expect("declared above");

        let mut fb = FunctionBuilder::new(
            mangled,
            fd.name,
            sig.ret.clone(),
            FunctionKind::Local,
        );
        fb.set_span(fd.span);
        let entry = fb.new_block();
        fb.switch_to(entry);

        let mut env = Env::default();
        let mut params_box = Vec::with_capacity(fd.params.len());
        for (param, pty) in fd.params.iter().zip(sig.params.iter()) {
            let v = fb.add_block_param(entry, pty.clone());
            env.bind(param.name, v, pty.clone());
            params_box.push(FuncParam {
                name: param.name,
                ty: pty.clone(),
                value: v,
            });
        }

        let mut __cellify: std::collections::HashSet<Symbol> = std::collections::HashSet::new();
        collect_cellified_names_block(&fd.body, &mut __cellify);
        let mut bcx = BodyCx {
            fb: &mut fb,
            env: &mut env,
            ret_ty: sig.ret.clone(),
            fn_ids: &mut self.fn_ids,
            fn_sigs: &mut self.fn_sigs,
            loops: Vec::new(),
            this_class: None,
            classes: &self.classes,
            class_meta: &self.class_meta,
            enum_ids: &self.enum_ids,
            enum_meta: &self.enum_meta,
            enums: &self.enums,
            statics: &self.statics,
            pending: &mut self.pending_closures,
            funcs: &mut self.funcs,
            anon_counter: &mut self.anon_counter,
            captures_in_scope: None,
            cell_captures: None,
            cellify_set: &__cellify,
            overloads: &self.overloads,
            repl_slots: &self.repl_slots,
            is_main_body: false,
            binding_self_name: None,
            crepr_owned_locals: std::collections::HashSet::new(),
        };
        let needs_retain = fd
            .body
            .tail
            .as_ref()
            .map(|e| bcx.callee_retain_decision(e))
            .unwrap_or(false);
        let tail = bcx.lower_block(&fd.body)?;
        if needs_retain {
            bcx.emit_callee_retain(&tail);
        }
        bcx.finalise_return(tail)?;

        let func = fb.finish(params_box.into_boxed_slice());
        self.funcs[id.0 as usize] = func;
        Ok(())
    }

    fn lower_main(&mut self, stmts: &[Stmt], tail: Option<&Expr>) -> Result<(), LowerError> {
        let main_name = Symbol::intern("__main");
        let mut fb = FunctionBuilder::new(main_name, main_name, MirTy::I64, FunctionKind::Local);
        let entry = fb.new_block();
        fb.switch_to(entry);

        let mut env = Env::default();

        // Lower statements then tail.
        let mut __cellify: std::collections::HashSet<Symbol> = std::collections::HashSet::new();
        for s in stmts {
            collect_cellified_names_stmt(s, &mut __cellify);
        }
        if let Some(t) = tail {
            collect_cellified_names_expr(t, &mut __cellify);
        }
        let mut bcx = BodyCx {
            fb: &mut fb,
            env: &mut env,
            ret_ty: MirTy::I64,
            fn_ids: &mut self.fn_ids,
            fn_sigs: &mut self.fn_sigs,
            loops: Vec::new(),
            this_class: None,
            classes: &self.classes,
            class_meta: &self.class_meta,
            enum_ids: &self.enum_ids,
            enum_meta: &self.enum_meta,
            enums: &self.enums,
            statics: &self.statics,
            pending: &mut self.pending_closures,
            funcs: &mut self.funcs,
            anon_counter: &mut self.anon_counter,
            captures_in_scope: None,
            cell_captures: None,
            cellify_set: &__cellify,
            overloads: &self.overloads,
            repl_slots: &self.repl_slots,
            is_main_body: true,
            binding_self_name: None,
            crepr_owned_locals: std::collections::HashSet::new(),
        };
        for stmt in stmts {
            bcx.lower_stmt(stmt)?;
        }
        let tail_val = match tail {
            Some(expr) => Some(bcx.lower_expr(expr)?),
            None => None,
        };

        // Release every top-level heap-typed `let` in reverse-bind
        // order so deinit fires before the process exits — matches
        // the existing `release_globals_at_exit` semantics.
        let top_scope: Vec<(Symbol, Binding)> = bcx
            .env
            .scopes
            .first()
            .cloned()
            .unwrap_or_default();
        let needs_release = |ty: &MirTy| {
            matches!(
                ty,
                MirTy::Object(_)
                    | MirTy::Fn(_)
                    | MirTy::Array { .. }
                    | MirTy::Optional(_)
                    | MirTy::Tuple(_)
                    | MirTy::Map { .. }
                    | MirTy::Str
                    | MirTy::Enum(_)
            )
        };
        for (_name, binding) in top_scope.into_iter().rev() {
            match binding {
                Binding::Local(lid, ty) if needs_release(&ty) => {
                    // CRepr Locals: only release the ones that own
                    // their backing buffer (mirrors the check in
                    // `release_top_scope_objects`). A `let p =
                    // r.origin` borrow stays alive with its
                    // parent struct.
                    if let MirTy::Object(cid) = &ty {
                        let layout = &bcx.classes[cid.0 as usize];
                        let is_crepr = matches!(
                            layout.repr,
                            crate::program::ClassRepr::CRepr
                                | crate::program::ClassRepr::CPacked
                                | crate::program::ClassRepr::CUnion
                        );
                        if is_crepr && !bcx.crepr_owned_locals.contains(&lid) {
                            continue;
                        }
                    }
                    let v = bcx.fb.new_value(ty.clone());
                    bcx.fb.push_inst(Inst::UseLocal { dst: v, local: lid });
                    bcx.fb.push_inst(Inst::Release { value: v });
                }
                Binding::Ssa(v, ty) if needs_release(&ty) => {
                    bcx.fb.push_inst(Inst::Release { value: v });
                }
                Binding::Cell(cell_v, ty) if needs_release(&ty) => {
                    let zero = bcx.const_int(MirTy::I64, 0);
                    let v = bcx.fb.new_value(ty.clone());
                    bcx.fb.push_inst(Inst::ArrayLoad {
                        dst: v,
                        arr: cell_v,
                        idx: zero,
                    });
                    bcx.fb.push_inst(Inst::Release { value: v });
                }
                _ => {}
            }
        }
        // Slot-backed top-level heap lets: balance the retain at
        // store-time with a release at process exit so any class
        // `deinit` fires before main returns. Emitted in
        // descending-slot order to mirror reverse-bind LIFO release.
        let mut slot_releases: Vec<(u32, MirTy)> = bcx
            .repl_slots
            .iter()
            .filter(|(_, (_, ty))| needs_release(ty))
            .map(|(_, (idx, ty))| (*idx, ty.clone()))
            .collect();
        slot_releases.sort_by(|a, b| b.0.cmp(&a.0));
        for (idx, ty) in slot_releases {
            let idx_v = bcx.const_int(MirTy::I64, idx as i64);
            let raw = bcx.fb.new_value(MirTy::I64);
            bcx.fb.push_inst(Inst::Call {
                dst: Some(raw),
                callee: FuncRef::Builtin(Symbol::intern("__repl_load_slot")),
                args: Box::new([idx_v]),
            });
            let v = bcx.i64_to_slot_value(raw, &ty)?;
            bcx.fb.push_inst(Inst::Release { value: v });
        }

        // __main returns `i64` (process exit code). If the program
        // tail is an i64 expression, return that; otherwise return 0.
        let ret_val = match tail_val {
            Some((v, MirTy::I64)) => v,
            _ => bcx.const_int(MirTy::I64, 0),
        };
        bcx.fb.set_terminator(Terminator::Return { value: Some(ret_val) });

        let func = fb.finish(Box::new([]));
        let id = FuncId(self.funcs.len() as u32);
        self.funcs.push(func);
        self.main_id = Some(id);
        Ok(())
    }
}




struct BodyCx<'a> {
    fb: &'a mut FunctionBuilder,
    env: &'a mut Env,
    ret_ty: MirTy,
    fn_ids: &'a mut HashMap<Symbol, FuncId>,
    fn_sigs: &'a mut HashMap<Symbol, FnSig>,
    loops: Vec<LoopFrame>,
    /// The receiver class when lowering a method body (`Some(cid)`).
    this_class: Option<crate::types::ClassId>,
    classes: &'a [crate::program::ClassLayout],
    class_meta: &'a HashMap<crate::types::ClassId, ClassMeta>,
    enum_ids: &'a HashMap<Symbol, crate::types::EnumId>,
    enum_meta: &'a HashMap<crate::types::EnumId, EnumMeta>,
    enums: &'a [crate::program::EnumLayout],
    statics: &'a [crate::program::StaticSlot],
    /// Slot for pushing newly-discovered anonymous closures that need
    /// their bodies lowered after the current fn finishes.
    pending: &'a mut Vec<PendingClosure>,
    funcs: &'a mut Vec<Function>,
    anon_counter: &'a mut u32,
    /// Captures available in this scope (only set when lowering a
    /// closure body — maps a captured name to its `LoadCapture(i)`
    /// index plus type).
    captures_in_scope: Option<&'a HashMap<Symbol, (u32, MirTy)>>,
    /// Names whose captures are heap cells (the cell pointer was
    /// captured, not the value snapshot). Reads / writes go through
    /// `ArrayLoad` / `ArrayStore` after a `LoadCapture` on the cell
    /// pointer.
    cell_captures: Option<&'a std::collections::HashSet<Symbol>>,
    overloads: &'a HashMap<Symbol, Vec<Symbol>>,
    /// Names that should be allocated as heap cells inside this fn
    /// body (because some inner closure captures+mutates them).
    /// Populated by a per-fn-body pre-pass.
    cellify_set: &'a std::collections::HashSet<Symbol>,
    /// REPL persistent slots: name → (slot index, MirTy). Forwarded
    /// from `Lower::repl_slots`. Drives `__repl_load_slot` emission
    /// in `Var` lookup (any fn body) and `__repl_store_slot` after
    /// top-level `let`s in `__main` when `is_main_body` is set.
    repl_slots: &'a HashMap<Symbol, (u32, MirTy)>,
    /// True iff we're lowering `__main`'s body. Restricts top-level
    /// `let` → slot-store to that scope so a same-named local in a
    /// fn body doesn't accidentally clobber the REPL slot.
    is_main_body: bool,
    /// Locals whose value is an owned `host_mir_alloc` buffer for a
    /// CRepr (no-rc-header) struct. Populated when a `let` binding
    /// stores a fresh `new T()` of a CRepr class. `release_top_scope
    /// _objects` consults this when emitting the scope-exit Release
    /// for CRepr Locals — without it, a `let p = r.origin` (where
    /// `r.origin` is just a borrow into `r`'s buffer) would
    /// erroneously free part of `r`'s memory.
    crepr_owned_locals: std::collections::HashSet<crate::inst::LocalId>,
    /// Name of the top-level slot binding currently being assigned
    /// (Some(X) while we're inside the value of `let X = ...`).
    /// `lower_fn_expr` checks this to avoid snapshotting the X slot
    /// when X appears as a free var inside the FnExpr body — that's
    /// the canonical self-recursive closure pattern, where the slot
    /// hasn't been written yet at construction time. The Var
    /// lookup inside the body resolves through the slot at call
    /// time instead.
    binding_self_name: Option<Symbol>,
}

impl<'a> BodyCx<'a> {
    fn statics_by_id(&self, id: crate::inst::StaticSlotId) -> crate::program::StaticSlot {
        self.statics[id.0 as usize].clone()
    }
    fn overloads_lookup(&self, name: Symbol) -> Option<Vec<Symbol>> {
        self.overloads.get(&name).cloned()
    }

    /// Bit-cast a value of `from` MirTy to a raw i64 for storage in a
    /// REPL slot. Heap pointers pass through; signed ints sextend;
    /// unsigned / bool zext; floats bitcast. Used by both the let-
    /// store path and any other slot-write site.
    fn value_to_i64(&mut self, v: ValueId, from: &MirTy) -> Result<ValueId, LowerError> {
        use crate::inst::CastKind;
        match from {
            MirTy::I64 | MirTy::U64 => Ok(v),
            MirTy::Object(_)
            | MirTy::Array { .. }
            | MirTy::Tuple(_)
            | MirTy::Map { .. }
            | MirTy::Optional(_)
            | MirTy::Fn(_)
            | MirTy::Str
            | MirTy::Enum(_)
            | MirTy::Weak(_)
            | MirTy::RawPtr { .. } => Ok(v),
            MirTy::I8 | MirTy::I16 | MirTy::I32 | MirTy::SSize => {
                let dst = self.fb.new_value(MirTy::I64);
                self.fb.push_inst(Inst::Cast { dst, kind: CastKind::IntResize, src: v });
                Ok(dst)
            }
            MirTy::U8 | MirTy::U16 | MirTy::U32 | MirTy::Size | MirTy::CChar | MirTy::Bool => {
                let dst = self.fb.new_value(MirTy::I64);
                // IntSignCross widens via uextend (zero-extend).
                self.fb.push_inst(Inst::Cast { dst, kind: CastKind::IntSignCross, src: v });
                Ok(dst)
            }
            MirTy::F64 | MirTy::F32 => {
                // No bitcast inst — funnel through the raw-ptr cast
                // which is a same-width identity at the clif level.
                // For F32 we'd lose the high bits; document as an
                // M1 limitation (REPL never round-trips f32 specially).
                let dst = self.fb.new_value(MirTy::I64);
                self.fb.push_inst(Inst::Cast { dst, kind: CastKind::PtrIntCast, src: v });
                Ok(dst)
            }
            MirTy::Unit => {
                // Unit slot: store a zero sentinel.
                Ok(self.const_int(MirTy::I64, 0))
            }
            MirTy::CVoid | MirTy::TypeVar(_) => Err(LowerError::Other(format!(
                "REPL slot store: unsupported type {from}"
            ))),
        }
    }

    /// Reverse of `value_to_i64` — narrow a raw i64 back to the slot's
    /// declared MirTy. Heap pointers reinterpret via PtrIntCast (a
    /// no-op at the bit level); primitives narrow via Cast.
    fn i64_to_slot_value(
        &mut self,
        raw: ValueId,
        to: &MirTy,
    ) -> Result<ValueId, LowerError> {
        use crate::inst::CastKind;
        match to {
            MirTy::I64 | MirTy::U64 => Ok(raw),
            MirTy::Object(_)
            | MirTy::Array { .. }
            | MirTy::Tuple(_)
            | MirTy::Map { .. }
            | MirTy::Optional(_)
            | MirTy::Fn(_)
            | MirTy::Str
            | MirTy::Enum(_)
            | MirTy::Weak(_)
            | MirTy::RawPtr { .. } => {
                let dst = self.fb.new_value(to.clone());
                self.fb.push_inst(Inst::Cast { dst, kind: CastKind::PtrIntCast, src: raw });
                Ok(dst)
            }
            MirTy::I8 | MirTy::I16 | MirTy::I32 | MirTy::SSize
            | MirTy::U8 | MirTy::U16 | MirTy::U32 | MirTy::Size | MirTy::CChar
            | MirTy::Bool => {
                let dst = self.fb.new_value(to.clone());
                self.fb.push_inst(Inst::Cast { dst, kind: CastKind::IntResize, src: raw });
                Ok(dst)
            }
            MirTy::F64 | MirTy::F32 => {
                let dst = self.fb.new_value(to.clone());
                self.fb.push_inst(Inst::Cast { dst, kind: CastKind::PtrIntCast, src: raw });
                Ok(dst)
            }
            MirTy::Unit => Ok(self.const_unit()),
            MirTy::CVoid | MirTy::TypeVar(_) => Err(LowerError::Other(format!(
                "REPL slot load: unsupported type {to}"
            ))),
        }
    }

    /// Resolve a name to its current value, emitting `UseLocal` for
    /// mutable bindings. Returns `None` if the name is unbound.
    fn lookup_var(&mut self, name: Symbol) -> Option<(ValueId, MirTy)> {
        match self.env.lookup_binding(name)? {
            Binding::Ssa(v, t) => Some((v, t)),
            Binding::Local(lid, t) => {
                let v = self.fb.new_value(t.clone());
                self.fb.push_inst(Inst::UseLocal { dst: v, local: lid });
                Some((v, t))
            }
            Binding::Cell(cell_v, t) => {
                let zero = self.const_int(MirTy::I64, 0);
                let v = self.fb.new_value(t.clone());
                self.fb.push_inst(Inst::ArrayLoad { dst: v, arr: cell_v, idx: zero });
                Some((v, t))
            }
        }
    }

    /// Look up the cell pointer (without dereferencing) for a Cell
    /// binding. Used at closure-capture sites so the closure shares
    /// the same heap cell with the outer scope.
    fn lookup_cell_ptr(&self, name: Symbol) -> Option<(ValueId, MirTy)> {
        match self.env.lookup_binding(name)? {
            Binding::Cell(cell_v, t) => Some((cell_v, t)),
            _ => None,
        }
    }

    /// Assign to an existing binding. Returns whether the binding
    /// existed. For Local bindings, emits a `DefLocal`. For Ssa
    /// bindings, replaces the slot's payload.
    fn assign_var(&mut self, name: Symbol, v: ValueId, ty: MirTy) -> bool {
        // The rhs's MirTy may be wider than the binding's declared
        // type after `unify_numeric` promoted a mixed-sign / mixed-
        // width arithmetic operand. `i = i + 1` (i: i32) is the
        // canonical case: `i + 1` widens to i64, but the Local was
        // declared i32, so a raw `def_var` would fail the
        // Cranelift type check. Insert a narrowing coerce when the
        // shapes don't already match.
        match self.env.lookup_binding(name) {
            Some(Binding::Local(lid, slot_ty)) => {
                let coerced = if slot_ty == ty {
                    v
                } else {
                    self.coerce(v, &ty, &slot_ty, Span::dummy()).unwrap_or(v)
                };
                self.fb
                    .push_inst(Inst::DefLocal { local: lid, value: coerced });
                true
            }
            Some(Binding::Cell(cell_v, slot_ty)) => {
                let coerced = if slot_ty == ty {
                    v
                } else {
                    self.coerce(v, &ty, &slot_ty, Span::dummy()).unwrap_or(v)
                };
                let zero = self.const_int(MirTy::I64, 0);
                // For heap-typed cells the cell owns the slot's rc:
                // release the previous occupant and retain the new
                // one. Without these, the old value's share leaks
                // (or worse, double-frees on later cell release) and
                // the new value's share goes unaccounted. Caught by
                // ASan as a UAF in `host_retain_object` while
                // re-assigning closure-captured Box instances.
                let heap_slot = matches!(
                    slot_ty,
                    MirTy::Object(_)
                        | MirTy::Fn(_)
                        | MirTy::Array { .. }
                        | MirTy::Optional(_)
                        | MirTy::Tuple(_)
                        | MirTy::Map { .. }
                        | MirTy::Str
                        | MirTy::Enum(_)
                );
                if heap_slot {
                    let old = self.fb.new_value(slot_ty.clone());
                    self.fb.push_inst(Inst::ArrayLoad {
                        dst: old,
                        arr: cell_v,
                        idx: zero,
                    });
                    self.fb.push_inst(Inst::Release { value: old });
                    self.fb.push_inst(Inst::Retain { value: coerced });
                }
                self.fb.push_inst(Inst::ArrayStore {
                    arr: cell_v,
                    idx: zero,
                    value: coerced,
                });
                true
            }
            Some(Binding::Ssa(_, _)) => {
                self.env.rebind(name, v, ty);
                true
            }
            None => false,
        }
    }
}

/// Emit `Inst::Retain` on `v` when `ty` is a heap-tracked MirTy.
/// Used by paths that store a value into a container the runtime
/// will later release on cascade (cell captures, etc.).

impl<'a> BodyCx<'a> {
    fn const_int(&mut self, ty: MirTy, n: i64) -> ValueId {
        let dst = self.fb.new_value(ty);
        self.fb.push_inst(Inst::Const { dst, value: MirConst::Int(n) });
        dst
    }
    fn const_unit(&mut self) -> ValueId {
        let dst = self.fb.new_value(MirTy::Unit);
        self.fb.push_inst(Inst::Const { dst, value: MirConst::Unit });
        dst
    }

    /// Standard refcount calling convention: callee returns +1 to
    /// caller. Three tail flavours need different handling:
    ///
    ///  (a) Fresh allocation (NewObject / Call / Binary on Str /
    ///      array literal / closure expr / …): rc=1 is already +1
    ///      for the caller. No retain.
    ///
    ///  (b) Var that resolves to a let-bound Local INSIDE the body
    ///      block: lower_block inserts a tail-alias retain to
    ///      balance the scope-exit release; the Local's +1
    ///      transfers to the caller. No extra retain.
    ///
    ///  (c) Var that resolves to an outer-scope binding (params
    ///      like `this`, captures) OR any non-Var aliased ref
    ///      (`this.field`, `arr[i]`, etc.): no +1 exists yet for
    ///      the caller — synthesise one so `c.inc()`-style chains
    ///      and `obj.field` returns hand the caller a real
    ///      ownership share. Without this the caller-side release
    ///      eventually frees while another binding still points at
    ///      the object.
    ///
    /// Returns true iff a callee-retain WILL be emitted for this
    /// tail; the actual emission must happen AFTER lower_block (so
    /// the ValueId is known) but the lookup runs BEFORE it (so the
    /// body block's let bindings haven't shadowed the outer scope
    /// yet — otherwise a tail Var that names a let-bound Local
    /// would lookup as "not Local" and we'd over-retain transient
    /// values like `make_map()`).
    fn callee_retain_decision(&self, tail_expr: &Expr) -> bool {
        if self.is_fresh_object_expr(tail_expr) {
            return false;
        }
        match &tail_expr.kind {
            ExprKind::Var(name) => match self.env.lookup_binding(*name) {
                // Resolves in the current (outer) scope — param or
                // earlier-block tail. Needs retain.
                Some(_) => true,
                // Doesn't resolve here ⇒ Var must be bound by a
                // `let` inside the body block, which lower_block
                // already retains for the caller.
                None => false,
            },
            _ => true,
        }
    }

    fn emit_callee_retain(&mut self, tail: &Option<(ValueId, MirTy)>) {
        if let Some((v, ty)) = tail.as_ref() {
            if matches!(
                ty,
                MirTy::Object(_)
                    | MirTy::Array { .. }
                    | MirTy::Tuple(_)
                    | MirTy::Map { .. }
                    | MirTy::Optional(_)
                    | MirTy::Fn(_)
                    | MirTy::Str
            ) {
                self.fb.push_inst(Inst::Retain { value: *v });
            }
        }
    }

    fn finalise_return(&mut self, tail: Option<(ValueId, MirTy)>) -> Result<(), LowerError> {
        // Synthesise a placeholder return value when the lowerer is
        // sitting in a dead block (the user already issued `return`
        // earlier on the dominating path) but the fn signature
        // expects a non-unit return.
        let synth_placeholder = |this: &mut Self, ret_ty: &MirTy| -> ValueId {
            let v = this.fb.new_value(ret_ty.clone());
            let c = match ret_ty {
                MirTy::Bool => Inst::Const { dst: v, value: MirConst::Bool(false) },
                MirTy::F32 => Inst::Const { dst: v, value: MirConst::F32(0) },
                MirTy::F64 => Inst::Const { dst: v, value: MirConst::F64(0) },
                _ => Inst::Const { dst: v, value: MirConst::Int(0) },
            };
            this.fb.push_inst(c);
            v
        };
        let value = match (&self.ret_ty, tail) {
            (MirTy::Unit, _) => None,
            // Tail expression has unit type (e.g. `return X` desugars
            // to a unit value in a dead block) — fabricate a real
            // return value so Cranelift's verifier is happy.
            (ret_ty, Some((_, MirTy::Unit))) => Some(synth_placeholder(self, &ret_ty.clone())),
            (ret_ty, Some((v, vty))) => {
                // Auto-coerce when the tail's type is a same-shape
                // integer / float that fits the declared return.
                let ret_ty_clone = ret_ty.clone();
                if vty == ret_ty_clone {
                    Some(v)
                } else {
                    let coerced = self.coerce(v, &vty, &ret_ty_clone, Span::dummy())
                        .unwrap_or(v);
                    Some(coerced)
                }
            }
            (ret_ty, None) => Some(synth_placeholder(self, &ret_ty.clone())),
        };
        self.fb.set_terminator(Terminator::Return { value });
        Ok(())
    }

    fn lower_block(&mut self, blk: &AstBlock) -> Result<Option<(ValueId, MirTy)>, LowerError> {
        self.env.enter_scope();
        // Stop treating let bindings as top-level once we descend
        // into a nested block — block-scoped `let x = ...` should
        // bind a fresh Local instead of overwriting any same-named
        // outer slot. lower_main calls `lower_stmt` directly on
        // its top-level stmts, so this flag flip only affects
        // recursion through `lower_expr(Block)`.
        let saved_main_body = self.is_main_body;
        self.is_main_body = false;
        for stmt in &blk.stmts {
            self.lower_stmt(stmt)?;
        }
        let tail = match &blk.tail {
            Some(e) => Some(self.lower_expr(e)?),
            None => None,
        };
        self.is_main_body = saved_main_body;
        // If the tail aliases a block-local heap binding, retain it
        // so the scope-exit releases below don't drop its rc to 0.
        // Fresh tails (`new T()` / call) are already +1 owners and
        // need no retain. Only retain when the tail expression is a
        // `Var` resolving to a binding in this block's scope —
        // otherwise we'd over-retain transient values that nothing
        // releases.
        let tail_needs_retain = |ty: &MirTy| {
            matches!(
                ty,
                MirTy::Object(_)
                    | MirTy::Fn(_)
                    | MirTy::Array { .. }
                    | MirTy::Optional(_)
                    | MirTy::Tuple(_)
                    | MirTy::Map { .. }
                    | MirTy::Str
                    | MirTy::Enum(_)
            )
        };
        let tail_alias_name = blk.tail.as_ref().and_then(|e| match &e.kind {
            ExprKind::Var(name) => Some(*name),
            _ => None,
        });
        let tail_aliases_local = tail_alias_name.is_some();
        let tail = match tail {
            Some((v, ty)) if tail_needs_retain(&ty) && tail_aliases_local => {
                self.fb.push_inst(Inst::Retain { value: v });
                Some((v, ty))
            }
            other => other,
        };
        // CRepr Locals carry no rc — Retain above is a no-op for
        // them. Transfer ownership of the tail-aliased local to
        // the caller by un-marking it before scope exit, otherwise
        // `release_top_scope_objects` would free the buffer the
        // caller is about to use.
        if let Some(name) = tail_alias_name {
            if let Some(Binding::Local(lid, _)) = self.env.lookup_binding(name) {
                self.crepr_owned_locals.remove(&lid);
            }
        }
        self.release_top_scope_objects();
        self.env.exit_scope();
        Ok(tail)
    }

    fn is_fresh_object_expr(&self, e: &Expr) -> bool {
        match &e.kind {
            ExprKind::New { .. }
            | ExprKind::StructLit { .. }
            | ExprKind::Call { .. }
            | ExprKind::MethodCall { .. }
            // SuperCall returns `this` aliased — init's calling
            // convention does NOT add a +1 (see lower_method's
            // is_init terminator special-case which sets the
            // terminator directly to `return this_v` with no
            // retain). Treating super() as fresh would emit a
            // bogus release-on-discard that drops rc below the
            // alloc's +1 and triggers free-during-init.
            // Binary / Unary on heap operands (string +) lowers to a
            // host helper (str_concat etc.) that returns a freshly
            // leak_cstring'd, registry-tracked buffer. Treating them
            // as fresh prevents the let-bind retain from leaking the
            // intermediate. For non-heap operand types, "fresh" is a
            // no-op decision so this is safe to widen unconditionally.
            | ExprKind::Binary { .. }
            | ExprKind::Unary { .. }
            // Aggregate / heap literals — each lowers to a fresh
            // alloc with rc=1 already in place.
            | ExprKind::Array(_)
            | ExprKind::Tuple(_)
            | ExprKind::Some(_)
            | ExprKind::None
            | ExprKind::EnumCtor { .. }
            | ExprKind::FnExpr { .. } => true,
            // Indexing a fresh tuple / array donates ownership of the
            // selected element to the caller — the lowerer retains
            // that element exactly once on the fresh-receiver path.
            ExprKind::Index { obj, .. } => self.is_fresh_object_expr(obj),
            // A block whose tail is itself fresh produces a fresh
            // value (the inner block scope-releases its own locals).
            ExprKind::Block(b) => b
                .tail
                .as_ref()
                .map(|t| self.is_fresh_object_expr(t))
                .unwrap_or(false),
            // `if`/`match` carry the freshness of all branches; treat
            // them conservatively — fresh only if every branch's tail
            // is fresh. Non-fresh would produce an over-retain rather
            // than a use-after-free, so this is the safe direction.
            ExprKind::If { then_branch, else_branch, .. } => {
                let then_fresh = then_branch
                    .tail
                    .as_ref()
                    .map(|t| self.is_fresh_object_expr(t))
                    .unwrap_or(false);
                let else_fresh = else_branch
                    .as_ref()
                    .map(|e| self.is_fresh_object_expr(e))
                    .unwrap_or(false);
                then_fresh && else_fresh
            }
            _ => false,
        }
    }

    fn release_top_scope_objects(&mut self) {
        let scope: Vec<(Symbol, Binding)> = self
            .env
            .scopes
            .last()
            .cloned()
            .unwrap_or_default();
        let needs_release = |ty: &MirTy| {
            matches!(
                ty,
                MirTy::Object(_)
                    | MirTy::Fn(_)
                    | MirTy::Array { .. }
                    | MirTy::Optional(_)
                    | MirTy::Tuple(_)
                    | MirTy::Map { .. }
                    | MirTy::Str
                    | MirTy::Enum(_)
            )
        };
        for (_name, binding) in scope.into_iter().rev() {
            match binding {
                Binding::Local(lid, ty) if needs_release(&ty) => {
                    // For CRepr Locals, only emit Release if this
                    // Local owns the underlying buffer. Borrowed
                    // CRepr values (e.g. nested-field reads) stay
                    // alive with their parent and must NOT be
                    // freed independently.
                    if let MirTy::Object(cid) = &ty {
                        let layout = &self.classes[cid.0 as usize];
                        let is_crepr = matches!(
                            layout.repr,
                            crate::program::ClassRepr::CRepr
                                | crate::program::ClassRepr::CPacked
                                | crate::program::ClassRepr::CUnion
                        );
                        if is_crepr && !self.crepr_owned_locals.contains(&lid) {
                            continue;
                        }
                    }
                    let v = self.fb.new_value(ty.clone());
                    self.fb.push_inst(Inst::UseLocal { dst: v, local: lid });
                    self.fb.push_inst(Inst::Release { value: v });
                }
                Binding::Ssa(v, ty) if needs_release(&ty) => {
                    self.fb.push_inst(Inst::Release { value: v });
                }
                Binding::Cell(cell_v, ty) if needs_release(&ty) => {
                    let zero = self.const_int(MirTy::I64, 0);
                    let v = self.fb.new_value(ty.clone());
                    self.fb.push_inst(Inst::ArrayLoad {
                        dst: v,
                        arr: cell_v,
                        idx: zero,
                    });
                    self.fb.push_inst(Inst::Release { value: v });
                }
                _ => {}
            }
        }
    }

    fn resolve_ty(&self, t: &Type) -> Result<MirTy, LowerError> {
        match t {
            Type::Object(name) => {
                // Find class first.
                if let Some((cid, _)) = self
                    .class_meta
                    .iter()
                    .find(|(cid, _)| self.classes[cid.0 as usize].name == *name)
                {
                    return Ok(MirTy::Object(*cid));
                }
                if let Some(eid) = self.enum_ids.get(name) {
                    return Ok(MirTy::Enum(*eid));
                }
                Err(LowerError::Other(format!("unknown type: {name}")))
            }
            Type::Enum(name) => self.enum_ids.get(name).copied().map(MirTy::Enum).ok_or_else(
                || LowerError::Other(format!("unknown enum {name}")),
            ),
            Type::Array { elem, fixed } => Ok(MirTy::Array {
                elem: Box::new(self.resolve_ty(elem)?),
                len: *fixed,
            }),
            Type::Tuple(elems) => {
                let mut out = Vec::with_capacity(elems.len());
                for e in elems.iter() {
                    out.push(self.resolve_ty(e)?);
                }
                Ok(MirTy::Tuple(out.into_boxed_slice()))
            }
            Type::Optional(inner) => Ok(MirTy::Optional(Box::new(self.resolve_ty(inner)?))),
            Type::Weak(inner) => match &**inner {
                Type::Object(cname) => self
                    .classes
                    .iter()
                    .find(|c| c.name == *cname)
                    .map(|c| MirTy::Weak(c.id))
                    .ok_or_else(|| LowerError::Other(format!("unknown class for weak: {cname}"))),
                _ => Err(LowerError::Other("`.weak` only applies to class types".into())),
            },
            Type::Generic(g) if g.base.as_str() == "Map" && g.args.len() == 2 => Ok(MirTy::Map {
                key: Box::new(self.resolve_ty(&g.args[0])?),
                val: Box::new(self.resolve_ty(&g.args[1])?),
            }),
            Type::Generic(g) => self
                .enum_ids
                .get(&g.base)
                .copied()
                .map(MirTy::Enum)
                .ok_or_else(|| LowerError::Unsupported("user-defined generic types")),
            Type::Fn(ft) => {
                let mut params = Vec::with_capacity(ft.params.len());
                for p in ft.params.iter() {
                    params.push(self.resolve_ty(p)?);
                }
                let ret = self.resolve_ty(&ft.ret)?;
                Ok(MirTy::Fn(Box::new(crate::types::MirFnTy {
                    params: params.into_boxed_slice(),
                    ret,
                })))
            }
            other => ty_to_mir(other),
        }
    }

    fn lower_stmt(&mut self, stmt: &Stmt) -> Result<(), LowerError> {
        match &stmt.kind {
            StmtKind::Let { name, ty, value, .. } => {
                // `let _ = expr` discards the result. Lower the
                // expression for its side effects, then drop a
                // fresh heap result so deinit / registry release
                // fires immediately instead of being deferred to
                // the enclosing scope's exit. A borrowed result
                // (non-fresh) needs no release — the source slot
                // still owns its +1.
                if name.as_str() == "_" {
                    let value_is_fresh = self.is_fresh_object_expr(value);
                    let (v, vty) = self.lower_expr(value)?;
                    if value_is_fresh && vty.is_heap() {
                        self.fb.push_inst(Inst::Release { value: v });
                    }
                    return Ok(());
                }
                // Empty-array literal uses the binding's annotated
                // element type so `let xs: string[] = []` typechecks
                // without a needs-coerce step that doesn't exist.
                let bind_hint = ty.as_ref().and_then(|t| self.resolve_ty(t).ok());
                let value_is_fresh_object = self.is_fresh_object_expr(value);
                // While lowering this let's value, mark `name` as the
                // currently-binding self name so a recursive FnExpr
                // body referencing `name` resolves through the slot
                // at call time instead of snapshotting the (still
                // unwritten) slot at construction.
                let saved_self = self.binding_self_name;
                if self.is_main_body && self.repl_slots.contains_key(name) {
                    self.binding_self_name = Some(*name);
                }
                let (v, mty) = if let (
                    ExprKind::Array(items),
                    Some(MirTy::Array { elem, len }),
                ) = (&value.kind, &bind_hint)
                {
                    if items.is_empty() {
                        let ty_full = MirTy::Array {
                            elem: elem.clone(),
                            len: *len,
                        };
                        let dst = self.fb.new_value(ty_full.clone());
                        self.fb.push_inst(Inst::NewArrayEmpty {
                            dst,
                            elem: (**elem).clone(),
                            fixed_len: *len,
                        });
                        (dst, ty_full)
                    } else {
                        // Hint-directed lowering: build an array whose
                        // element MirTy AND fixed-length match the
                        // binding's hint, so the inline-vs-dynamic
                        // codegen layout is consistent with how
                        // ArrayLoad / ArrayLen later type-dispatch.
                        self.lower_array_literal_with_hint(
                            items,
                            Some((**elem).clone()),
                            *len,
                        )?
                    }
                } else {
                    self.lower_expr(value)?
                };
                let bind_ty = bind_hint.unwrap_or_else(|| mty.clone());
                let bound = if bind_ty != mty {
                    self.coerce(v, &mty, &bind_ty, stmt.span)?
                } else {
                    v
                };
                // For an aliased heap value (anything that isn't a
                // freshly-constructed `new T(...)` / closure expr /
                // literal), bump refcount — the binding shares
                // ownership with the source. All heap kinds (incl.
                // Array, Tuple, Map, Optional, Enum) need this so
                // the slot's scope-exit release has its own +1 to
                // drop; without it a container that releases the
                // element on overwrite (e.g. host_map_set's
                // release_by_kind) would free the buffer the slot
                // still points at.
                if bind_ty.is_heap() && !value_is_fresh_object {
                    self.fb.push_inst(Inst::Retain { value: bound });
                }
                // Slot-backed top-level binding: skip the local
                // entirely so all reads / writes (in `__main` *and*
                // any fn body) funnel through `__repl_load_slot` /
                // `__repl_store_slot`. Without this skip, `__main`
                // would read its own private Local copy and miss
                // mutations that other fns wrote through the slot.
                // `is_main_body` is cleared by `lower_block` on
                // descent so block-scoped `let x = 100` shadows
                // bind a fresh Local instead of overwriting the
                // outer slot.
                let is_slot_global = self.is_main_body
                    && self.repl_slots.contains_key(name);
                if matches!(bind_ty, MirTy::Unit) {
                    // Unit-typed bindings have no clif representation;
                    // keep the SSA path so reads return a synthetic
                    // unit value.
                    self.env.bind(*name, bound, bind_ty.clone());
                } else if is_slot_global {
                    // No local binding — slot lookup handles reads.
                } else {
                    let _ = &self.cellify_set; // legacy field, retained for ABI
                    let lid = self.fb.new_local(bind_ty.clone());
                    self.fb.push_inst(Inst::DefLocal { local: lid, value: bound });
                    self.env.bind_local(*name, lid, bind_ty.clone());
                    // Mark CRepr Locals as "owns the buffer" only
                    // when the source was a fresh `new T()` (or
                    // similar) — that's what makes the buffer
                    // safe to free at scope exit. A `let p =
                    // r.origin` style borrow stays unmarked so
                    // the scope-exit path leaves it alone.
                    if let MirTy::Object(cid) = &bind_ty {
                        let layout = &self.classes[cid.0 as usize];
                        if matches!(
                            layout.repr,
                            crate::program::ClassRepr::CRepr
                                | crate::program::ClassRepr::CPacked
                                | crate::program::ClassRepr::CUnion
                        ) && value_is_fresh_object
                        {
                            self.crepr_owned_locals.insert(lid);
                        }
                    }
                }
                // REPL: top-level let in __main with a registered slot
                // → persist the value to a host-side cell so future
                // chunks can read it via `__repl_load_slot`.
                if self.is_main_body {
                    if let Some((idx, _slot_ty)) = self.repl_slots.get(name).cloned() {
                        let idx_v = self.const_int(MirTy::I64, idx as i64);
                        // Bit-cast the bound value to i64 for storage.
                        // Heap pointer types are already i64; signed
                        // ints widen via sextend; unsigned ints / bool
                        // via zext; floats via bitcast.
                        let v_i64 = self.value_to_i64(bound, &bind_ty)?;
                        // The slot becomes the only owner of the
                        // value (slot-promoted top-level lets get NO
                        // Local binding above, so __main's exit
                        // release sweep doesn't touch the name).
                        // Aliased heap values need a fresh +1 the
                        // slot can own; fresh values already come
                        // with rc=1, so retaining again leaves rc=2
                        // and the exit-time slot release can't drive
                        // the value to drop. See
                        // top_level_let_used_in_fn_deinit_once.il.
                        if matches!(
                            bind_ty,
                            MirTy::Object(_)
                                | MirTy::Array { .. }
                                | MirTy::Tuple(_)
                                | MirTy::Map { .. }
                                | MirTy::Optional(_)
                                | MirTy::Fn(_)
                        ) && !value_is_fresh_object
                        {
                            self.fb.push_inst(Inst::Retain { value: bound });
                        }
                        self.fb.push_inst(Inst::Call {
                            dst: None,
                            callee: FuncRef::Builtin(Symbol::intern("__repl_store_slot")),
                            args: Box::new([idx_v, v_i64]),
                        });
                    }
                }
                self.binding_self_name = saved_self;
                Ok(())
            }
            StmtKind::LetTuple { elems, value } => {
                let (v, vty) = self.lower_expr(value)?;
                let tuple_tys = match &vty {
                    MirTy::Tuple(ts) => ts.clone(),
                    other => {
                        return Err(LowerError::Other(format!(
                            "let-tuple destructure on non-tuple: {other}"
                        )))
                    }
                };
                if elems.len() != tuple_tys.len() {
                    return Err(LowerError::Other(format!(
                        "tuple destructure arity {} vs tuple {}",
                        elems.len(),
                        tuple_tys.len()
                    )));
                }
                for (i, name_opt) in elems.iter().enumerate() {
                    let Some(name) = name_opt else { continue };
                    let ty = tuple_tys[i].clone();
                    let dst = self.fb.new_value(ty.clone());
                    self.fb.push_inst(Inst::TupleExtract {
                        dst,
                        tup: v,
                        idx: i as u32,
                    });
                    self.env.bind(*name, dst, ty);
                }
                Ok(())
            }
            StmtKind::LetStruct { class, fields, value } => {
                let (v, vty) = self.lower_expr(value)?;
                let class_id = match &vty {
                    MirTy::Object(c) => *c,
                    other => {
                        return Err(LowerError::Other(format!(
                            "let-struct destructure on non-object: {other}"
                        )))
                    }
                };
                let layout = &self.classes[class_id.0 as usize];
                if layout.name != *class {
                    return Err(LowerError::Other(format!(
                        "destructure class mismatch: declared {class}, value class {}",
                        layout.name
                    )));
                }
                let meta = self.class_meta.get(&class_id).expect("class meta");
                for fname in fields.iter() {
                    let &fid = meta.field_ix.get(fname).ok_or_else(|| {
                        LowerError::Other(format!("no field {fname} on {class}"))
                    })?;
                    let fty = meta.field_ty.get(&fid).cloned().unwrap();
                    let dst = self.fb.new_value(fty.clone());
                    self.fb.push_inst(Inst::LoadField { dst, obj: v, field: fid });
                    self.env.bind(*fname, dst, fty);
                }
                Ok(())
            }
            StmtKind::Expr(e) => {
                let (v, ty) = self.lower_expr(e)?;
                // If the expression-statement produced a fresh,
                // unowned heap value, release it so its refcount
                // drops to 0 (firing class deinit / freeing the
                // backing buffer). Without this, a discarded
                // method call result like `xs.map(fn(...){...})`
                // (its fresh array, plus the fresh closure arg)
                // leaks every iteration of a long-running loop.
                let is_heap = matches!(
                    ty,
                    MirTy::Object(_)
                        | MirTy::Array { .. }
                        | MirTy::Tuple(_)
                        | MirTy::Map { .. }
                        | MirTy::Optional(_)
                        | MirTy::Fn(_)
                );
                if is_heap && self.is_fresh_object_expr(e) {
                    self.fb.push_inst(Inst::Release { value: v });
                }
                Ok(())
            }
        }
    }

    fn lower_expr(&mut self, expr: &Expr) -> Result<(ValueId, MirTy), LowerError> {
        match &expr.kind {
            ExprKind::Int(n) => {
                let v = self.fb.new_value(MirTy::I64);
                self.fb.push_inst(Inst::Const { dst: v, value: MirConst::Int(*n) });
                Ok((v, MirTy::I64))
            }
            ExprKind::Bool(b) => {
                let v = self.fb.new_value(MirTy::Bool);
                self.fb.push_inst(Inst::Const { dst: v, value: MirConst::Bool(*b) });
                Ok((v, MirTy::Bool))
            }
            ExprKind::Float(f) => {
                let v = self.fb.new_value(MirTy::F64);
                self.fb.push_inst(Inst::Const { dst: v, value: MirConst::F64(f.to_bits()) });
                Ok((v, MirTy::F64))
            }
            ExprKind::Str(s) => {
                let v = self.fb.new_value(MirTy::Str);
                self.fb.push_inst(Inst::Const {
                    dst: v,
                    value: MirConst::Str(Symbol::intern(s)),
                });
                Ok((v, MirTy::Str))
            }
            ExprKind::Var(name) => {
                if let Some(found) = self.lookup_var(*name) {
                    return Ok(found);
                }
                // Closure capture takes precedence over a same-named
                // module slot — a closure that captured `factor`
                // when it was 10 must keep seeing 10 even if the
                // outer code reassigned the slot to 999.
                // Slot reads borrow ownership from the slot itself
                // (the host store keeps a stable refcount on the
                // slot's heap value). We deliberately do NOT retain
                // here — that's the same contract `lookup_var`
                // honours for Local reads, and it's what avoids the
                // per-loop-iteration leak in long-running programs
                // (e.g. `examples/sdl_breakout`'s game loop, where
                // every frame reads slot-promoted globals dozens of
                // times). Downstream `let`-binding / fn-arg /
                // closure-capture sites bump the refcount when they
                // need persistent ownership.
                if let Some(caps) = self.captures_in_scope {
                    if caps.contains_key(name) {
                        // Fall through to the existing capture
                        // handler below.
                    } else if let Some((idx, slot_ty)) = self.repl_slots.get(name).cloned() {
                        let idx_v = self.const_int(MirTy::I64, idx as i64);
                        let raw = self.fb.new_value(MirTy::I64);
                        self.fb.push_inst(Inst::Call {
                            dst: Some(raw),
                            callee: FuncRef::Builtin(Symbol::intern("__repl_load_slot")),
                            args: Box::new([idx_v]),
                        });
                        let v = self.i64_to_slot_value(raw, &slot_ty)?;
                        return Ok((v, slot_ty));
                    }
                } else if let Some((idx, slot_ty)) = self.repl_slots.get(name).cloned() {
                    // Non-closure context (regular fn body or
                    // `__main` itself): always go through the slot.
                    let idx_v = self.const_int(MirTy::I64, idx as i64);
                    let raw = self.fb.new_value(MirTy::I64);
                    self.fb.push_inst(Inst::Call {
                        dst: Some(raw),
                        callee: FuncRef::Builtin(Symbol::intern("__repl_load_slot")),
                        args: Box::new([idx_v]),
                    });
                    let v = self.i64_to_slot_value(raw, &slot_ty)?;
                    return Ok((v, slot_ty));
                }
                // Closure capture (only when lowering a closure body).
                if let Some(caps) = self.captures_in_scope {
                    if let Some((idx, cty)) = caps.get(name).cloned() {
                        let is_cell = self
                            .cell_captures
                            .map(|s| s.contains(name))
                            .unwrap_or(false);
                        if is_cell {
                            // Capture slot holds a cell pointer (i64
                            // 1-elem array). Load the pointer, then
                            // dereference to get the inner value.
                            let cell_v = self.fb.new_value(MirTy::I64);
                            self.fb.push_inst(Inst::LoadCapture { dst: cell_v, idx });
                            let zero = self.const_int(MirTy::I64, 0);
                            let v = self.fb.new_value(cty.clone());
                            self.fb.push_inst(Inst::ArrayLoad {
                                dst: v,
                                arr: cell_v,
                                idx: zero,
                            });
                            return Ok((v, cty));
                        }
                        let v = self.fb.new_value(cty.clone());
                        self.fb.push_inst(Inst::LoadCapture { dst: v, idx });
                        return Ok((v, cty));
                    }
                }
                // Top-level fn used as a value: produce a trampoline
                // closure with no captures.
                if let Some(&fid) = self.fn_ids.get(name) {
                    let sig = self.fn_sigs.get(name).cloned().unwrap();
                    let fn_ty = MirTy::Fn(Box::new(crate::types::MirFnTy {
                        params: sig.params.clone().into_boxed_slice(),
                        ret: sig.ret,
                    }));
                    let dst = self.fb.new_value(fn_ty.clone());
                    self.fb.push_inst(Inst::MakeClosure {
                        dst,
                        func: fid,
                        captures: Box::new([]),
                    });
                    return Ok((dst, fn_ty));
                }
                // Implicit `this.field` / `this.method()` — only the
                // field case applies here (method ref without call is
                // not supported in M1).
                if let Some(cid) = self.this_class {
                    let meta = self.class_meta.get(&cid).expect("class meta");
                    if let Some(&fid) = meta.field_ix.get(name) {
                        let (this_v, _) = self.lookup_var(Symbol::intern("this")).unwrap();
                        let fty = meta.field_ty.get(&fid).cloned().unwrap();
                        let v = self.fb.new_value(fty.clone());
                        self.fb.push_inst(Inst::LoadField { dst: v, obj: this_v, field: fid });
                        return Ok((v, fty));
                    }
                }
                Err(LowerError::Other(format!("unbound variable: {name}")))
            }
            ExprKind::This => {
                let this_sym = Symbol::intern("this");
                if let Some(found) = self.lookup_var(this_sym) {
                    return Ok(found);
                }
                if let Some(caps) = self.captures_in_scope {
                    if let Some((idx, cty)) = caps.get(&this_sym).cloned() {
                        let v = self.fb.new_value(cty.clone());
                        self.fb.push_inst(Inst::LoadCapture { dst: v, idx });
                        return Ok((v, cty));
                    }
                }
                Err(LowerError::Other("`this` outside method body".into()))
            }
            ExprKind::SuperCall { method, args } => self.lower_super_call(*method, args, expr.span),
            ExprKind::New { class, type_args, args, init_method } => {
                // Built-in `Map<K, V>` — `new Map<K,V>()` constructs
                // an empty map.
                if class.as_str() == "Map" && type_args.len() == 2 && args.is_empty() {
                    let key = self.resolve_ty(&type_args[0])?;
                    let val = self.resolve_ty(&type_args[1])?;
                    let ty = MirTy::Map {
                        key: Box::new(key.clone()),
                        val: Box::new(val.clone()),
                    };
                    let dst = self.fb.new_value(ty.clone());
                    self.fb.push_inst(Inst::NewMap {
                        dst,
                        key,
                        val,
                        entries: Box::new([]),
                    });
                    return Ok((dst, ty));
                }
                if !type_args.is_empty() {
                    return Err(LowerError::Unsupported("generic class instantiation"));
                }
                self.lower_new(*class, args, *init_method)
            }
            ExprKind::AssignField { obj, field, value, is_init } => {
                // `ClassName.field = v` on a static slot.
                if let ExprKind::Var(maybe_class) = &obj.kind {
                    if self.lookup_var(*maybe_class).is_none() {
                        if let Some(cid) = self.class_meta.iter().find_map(|(cid, _)| {
                            if self.classes[cid.0 as usize].name == *maybe_class {
                                Some(*cid)
                            } else {
                                None
                            }
                        }) {
                            let meta = self.class_meta.get(&cid).unwrap();
                            if let Some(&slot) = meta.static_slots.get(field) {
                                let s = self.statics_by_id(slot);
                                if s.is_const && !*is_init {
                                    return Err(LowerError::Other(format!(
                                        "cannot assign to const {field}"
                                    )));
                                }
                                let (vv, vty) = self.lower_expr(value)?;
                                let coerced = if vty == s.ty {
                                    vv
                                } else {
                                    self.coerce(vv, &vty, &s.ty, expr.span)?
                                };
                                self.fb.push_inst(Inst::StoreStatic { slot, value: coerced });
                                return Ok((self.const_unit(), MirTy::Unit));
                            }
                        }
                    }
                }
                let (ov, oty) = self.lower_expr(obj)?;
                let class_id = match &oty {
                    MirTy::Object(c) => *c,
                    other => {
                        return Err(LowerError::Other(format!(
                            "field assignment on non-object: {other}"
                        )))
                    }
                };
                // Property setter on instance.
                let meta = self.class_meta.get(&class_id).expect("class meta");
                if let Some((fid, prop_ty)) = meta.property_setter.get(field).cloned() {
                    let (vv, vty) = self.lower_expr(value)?;
                    let coerced = if vty == prop_ty {
                        vv
                    } else {
                        self.coerce(vv, &vty, &prop_ty, value.span)?
                    };
                    self.fb.push_inst(Inst::Call {
                        dst: None,
                        callee: FuncRef::Local(fid),
                        args: Box::new([ov, coerced]),
                    });
                    return Ok((self.const_unit(), MirTy::Unit));
                }
                let fid = *meta
                    .field_ix
                    .get(field)
                    .ok_or_else(|| LowerError::Other(format!("no field {field}")))?;
                let fty = meta.field_ty.get(&fid).cloned().unwrap_or(MirTy::I64);
                let value_is_fresh = self.is_fresh_object_expr(value);
                let (vv, _) = self.lower_expr(value)?;
                // ARC for any heap-typed field: retain the incoming
                // value (unless it was a fresh allocation that
                // already owned its +1) and release the previous
                // occupant. Without this, `this.balls = newArr` etc.
                // leaks the prior array's refcount on every frame
                // of `examples/sdl_breakout`'s game loop.
                let is_heap = matches!(
                    fty,
                    MirTy::Object(_)
                        | MirTy::Array { .. }
                        | MirTy::Tuple(_)
                        | MirTy::Map { .. }
                        | MirTy::Optional(_)
                        | MirTy::Fn(_)
                );
                if is_heap {
                    if !value_is_fresh {
                        self.fb.push_inst(Inst::Retain { value: vv });
                    }
                    let old = self.fb.new_value(fty.clone());
                    self.fb.push_inst(Inst::LoadField {
                        dst: old,
                        obj: ov,
                        field: fid,
                    });
                    self.fb.push_inst(Inst::Release { value: old });
                }
                self.fb.push_inst(Inst::StoreField { obj: ov, field: fid, value: vv });
                Ok((self.const_unit(), MirTy::Unit))
            }
            ExprKind::Unary { op, expr } => self.lower_unary(*op, expr, expr.span),
            ExprKind::Binary { op, lhs, rhs } => self.lower_binary(*op, lhs, rhs, expr.span),
            ExprKind::Logical { op, lhs, rhs } => self.lower_logical(*op, lhs, rhs),
            ExprKind::Block(b) => {
                let tail = self.lower_block(b)?;
                Ok(tail.unwrap_or_else(|| (self.const_unit(), MirTy::Unit)))
            }
            ExprKind::If { cond, then_branch, else_branch } => {
                self.lower_if(cond, then_branch, else_branch.as_deref())
            }
            ExprKind::While { cond, body } => self.lower_while(cond, body),
            ExprKind::Loop { body } => self.lower_loop(body),
            ExprKind::Break(v) => self.lower_break(v.as_deref()),
            ExprKind::Continue => self.lower_continue(),
            ExprKind::Return(v) => self.lower_return(v.as_deref()),
            ExprKind::Assign { target, value } => {
                let value_is_fresh = self.is_fresh_object_expr(value);
                // Snapshot the old value if the target binds an Object
                // — we need to Release it after the new value lands.
                let old_obj = self.env.lookup_binding(*target).and_then(|b| match b {
                    Binding::Local(lid, ty) if matches!(ty, MirTy::Object(_)) => {
                        let v = self.fb.new_value(ty.clone());
                        self.fb.push_inst(Inst::UseLocal { dst: v, local: lid });
                        Some(v)
                    }
                    Binding::Cell(cell_v, ty) if matches!(ty, MirTy::Object(_)) => {
                        let zero = self.const_int(MirTy::I64, 0);
                        let v = self.fb.new_value(ty.clone());
                        self.fb.push_inst(Inst::ArrayLoad {
                            dst: v,
                            arr: cell_v,
                            idx: zero,
                        });
                        Some(v)
                    }
                    _ => None,
                });
                let (v, vty) = self.lower_expr(value)?;
                if self.assign_var(*target, v, vty.clone()) {
                    if matches!(vty, MirTy::Object(_)) {
                        if !value_is_fresh {
                            self.fb.push_inst(Inst::Retain { value: v });
                        }
                        if let Some(old) = old_obj {
                            self.fb.push_inst(Inst::Release { value: old });
                        }
                    }
                    return Ok((self.const_unit(), MirTy::Unit));
                }
                // Closure capture assign: cell capture stores via
                // the loaded cell pointer.
                if let Some(caps) = self.captures_in_scope {
                    if let Some((idx, cty)) = caps.get(target).cloned() {
                        let is_cell = self
                            .cell_captures
                            .map(|s| s.contains(target))
                            .unwrap_or(false);
                        if is_cell {
                            let cell_v = self.fb.new_value(MirTy::I64);
                            self.fb.push_inst(Inst::LoadCapture { dst: cell_v, idx });
                            let zero = self.const_int(MirTy::I64, 0);
                            // Heap-typed cell: the cell owns the slot's
                            // share, so swap rc accounts before storing
                            // — release the previous occupant, retain
                            // the incoming. Without this the prior
                            // value's rc leaks (or, if the cell is
                            // re-overwritten later, the surviving
                            // alias double-frees). ASan caught the UAF
                            // in `host_retain_object` on the
                            // closure_swap_heap_capture fixture.
                            let heap_slot = matches!(
                                cty,
                                MirTy::Object(_)
                                    | MirTy::Fn(_)
                                    | MirTy::Array { .. }
                                    | MirTy::Optional(_)
                                    | MirTy::Tuple(_)
                                    | MirTy::Map { .. }
                                    | MirTy::Str
                                    | MirTy::Enum(_)
                            );
                            if heap_slot {
                                let old = self.fb.new_value(cty.clone());
                                self.fb.push_inst(Inst::ArrayLoad {
                                    dst: old,
                                    arr: cell_v,
                                    idx: zero,
                                });
                                self.fb.push_inst(Inst::Release { value: old });
                                self.fb.push_inst(Inst::Retain { value: v });
                            }
                            self.fb.push_inst(Inst::ArrayStore {
                                arr: cell_v,
                                idx: zero,
                                value: v,
                            });
                            return Ok((self.const_unit(), MirTy::Unit));
                        }
                    }
                }
                // REPL / module-global slot assign: when a fn body
                // mutates a top-level let from a `use`d module
                // (e.g. `state = state ^ ...` inside `rng.randNext`,
                // where the loader rewrote `state` to `rng.state`
                // and that name isn't in any local scope), route the
                // write through `__repl_store_slot`.
                if let Some((idx, slot_ty)) = self.repl_slots.get(target).cloned() {
                    let coerced = if vty == slot_ty {
                        v
                    } else {
                        self.coerce(v, &vty, &slot_ty, expr.span)?
                    };
                    let is_heap = matches!(
                        slot_ty,
                        MirTy::Object(_)
                            | MirTy::Array { .. }
                            | MirTy::Tuple(_)
                            | MirTy::Map { .. }
                            | MirTy::Optional(_)
                            | MirTy::Fn(_)
                    );
                    // Snapshot the prior slot value so the old heap
                    // owner gets released after the new value lands.
                    let old_v = if is_heap {
                        let idx_v = self.const_int(MirTy::I64, idx as i64);
                        let raw = self.fb.new_value(MirTy::I64);
                        self.fb.push_inst(Inst::Call {
                            dst: Some(raw),
                            callee: FuncRef::Builtin(Symbol::intern("__repl_load_slot")),
                            args: Box::new([idx_v]),
                        });
                        Some(self.i64_to_slot_value(raw, &slot_ty)?)
                    } else {
                        None
                    };
                    if is_heap && !value_is_fresh {
                        self.fb.push_inst(Inst::Retain { value: coerced });
                    }
                    let v_i64 = self.value_to_i64(coerced, &slot_ty)?;
                    let idx_v = self.const_int(MirTy::I64, idx as i64);
                    self.fb.push_inst(Inst::Call {
                        dst: None,
                        callee: FuncRef::Builtin(Symbol::intern("__repl_store_slot")),
                        args: Box::new([idx_v, v_i64]),
                    });
                    if let Some(old) = old_v {
                        self.fb.push_inst(Inst::Release { value: old });
                    }
                    return Ok((self.const_unit(), MirTy::Unit));
                }
                // Try implicit `this.<target>` field assignment.
                if let Some(cid) = self.this_class {
                    let meta = self.class_meta.get(&cid).expect("class meta");
                    if let Some(&fid) = meta.field_ix.get(target) {
                        let (this_v, _) = self.lookup_var(Symbol::intern("this")).unwrap();
                        // Heap field write: take ownership of `value`
                        // (retain if aliased) and release whatever was
                        // there before (if any). Covers every heap
                        // type — Object / Array / Tuple / Map /
                        // Optional / Fn — so re-assigning a field
                        // doesn't leak the prior occupant. Crucial
                        // for game-loop code that swaps arrays /
                        // optionals on every frame.
                        let value_is_fresh = self.is_fresh_object_expr(value);
                        let is_heap = matches!(
                            vty,
                            MirTy::Object(_)
                                | MirTy::Array { .. }
                                | MirTy::Tuple(_)
                                | MirTy::Map { .. }
                                | MirTy::Optional(_)
                                | MirTy::Fn(_)
                        );
                        if is_heap {
                            if !value_is_fresh {
                                self.fb.push_inst(Inst::Retain { value: v });
                            }
                            // Read old value and release it. Skips on
                            // null (init path).
                            let old = self.fb.new_value(vty.clone());
                            self.fb.push_inst(Inst::LoadField {
                                dst: old,
                                obj: this_v,
                                field: fid,
                            });
                            self.fb.push_inst(Inst::Release { value: old });
                        }
                        self.fb
                            .push_inst(Inst::StoreField { obj: this_v, field: fid, value: v });
                        return Ok((self.const_unit(), MirTy::Unit));
                    }
                }
                Err(LowerError::Other(format!(
                    "assignment to undefined variable: {target}"
                )))
            }
            ExprKind::Call { callee, args } => self.lower_call(*callee, args),
            ExprKind::Cast { expr: inner, ty } => {
                let (v, src_ty) = self.lower_expr(inner)?;
                let dst_ty = self.resolve_ty(ty)?;
                let out = self.coerce(v, &src_ty, &dst_ty, expr.span)?;
                Ok((out, dst_ty))
            }
            ExprKind::TypeTest { expr: inner, ty } => {
                let (v, _) = self.lower_expr(inner)?;
                let dst_ty = self.resolve_ty(ty)?;
                let class = match &dst_ty {
                    MirTy::Object(c) => *c,
                    other => {
                        return Err(LowerError::Other(format!(
                            "`is` requires a class type, got {other}"
                        )))
                    }
                };
                let dst = self.fb.new_value(MirTy::Bool);
                self.fb.push_inst(Inst::IsInstance { dst, value: v, class });
                Ok((dst, MirTy::Bool))
            }
            ExprKind::TypeDowncast { expr: inner, ty } => {
                let (v, _) = self.lower_expr(inner)?;
                let dst_ty = self.resolve_ty(ty)?;
                let class = match &dst_ty {
                    MirTy::Object(c) => *c,
                    other => {
                        return Err(LowerError::Other(format!(
                            "`as?` requires a class type, got {other}"
                        )))
                    }
                };
                let opt_ty = MirTy::Optional(Box::new(MirTy::Object(class)));
                let dst = self.fb.new_value(opt_ty.clone());
                self.fb.push_inst(Inst::DowncastOrNone { dst, value: v, class });
                Ok((dst, opt_ty))
            }
            ExprKind::Array(items) => self.lower_array_literal(items),
            ExprKind::Tuple(items) => self.lower_tuple_literal(items),
            ExprKind::None => {
                // `none` has no concrete T?; the binding context (let
                // annotation, function return type) decides. Until we
                // thread that through, materialise as `Optional<Unit>`.
                // Callers usually overwrite via coerce or fix the type
                // from the let annotation.
                let inner = MirTy::Unit;
                let ty = MirTy::Optional(Box::new(inner));
                let v = self.fb.new_value(ty.clone());
                self.fb.push_inst(Inst::Const { dst: v, value: MirConst::None });
                Ok((v, ty))
            }
            ExprKind::Some(inner) => {
                let value_is_fresh = self.is_fresh_object_expr(inner);
                let (iv, ity) = self.lower_expr(inner)?;
                // `some(arr)` where `arr` is an aliased Var that the
                // surrounding scope is about to release — bump the
                // inner's rc so the Optional doesn't dangle once the
                // source binding's scope-exit Release fires. With
                // host_release_array now actually freeing memory at
                // rc==0, omitting this retain caused use-after-free
                // in some_aliased_inner.il.
                let needs_retain = !value_is_fresh
                    && matches!(
                        ity,
                        MirTy::Object(_)
                            | MirTy::Array { .. }
                            | MirTy::Tuple(_)
                            | MirTy::Map { .. }
                            | MirTy::Optional(_)
                            | MirTy::Fn(_)
                            | MirTy::Str
                    );
                if needs_retain {
                    self.fb.push_inst(Inst::Retain { value: iv });
                }
                let ty = MirTy::Optional(Box::new(ity.clone()));
                let v = self.fb.new_value(ty.clone());
                self.fb.push_inst(Inst::NewOptional { dst: v, value: iv });
                Ok((v, ty))
            }
            ExprKind::Index { obj, index } => self.lower_index(obj, index),
            ExprKind::AssignIndex { obj, index, value } => {
                let value_is_fresh = self.is_fresh_object_expr(value);
                let (av, aty) = self.lower_expr(obj)?;
                let (iv, _) = self.lower_expr(index)?;
                let (vv, vty) = self.lower_expr(value)?;
                match aty {
                    MirTy::Array { ref elem, .. } => {
                        let elem_ty = (**elem).clone();
                        // Same shape as `StoreField` for heap elements:
                        // retain the incoming value (unless it owns its
                        // +1) and release whatever currently sits in the
                        // slot, since `__release_array`'s cascade
                        // already accounts for every stored element on
                        // drop. Without this, `arr[i] = borrowed` would
                        // share rc with the source slot (UAF) and
                        // `arr[i] = fresh` would leak the old occupant.
                        let elem_is_heap = matches!(
                            elem_ty,
                            MirTy::Object(_)
                                | MirTy::Array { .. }
                                | MirTy::Tuple(_)
                                | MirTy::Map { .. }
                                | MirTy::Optional(_)
                                | MirTy::Fn(_)
                                | MirTy::Str
                                | MirTy::Enum(_)
                        );
                        if elem_is_heap {
                            if !value_is_fresh {
                                self.fb.push_inst(Inst::Retain { value: vv });
                            }
                            let old = self.fb.new_value(elem_ty.clone());
                            self.fb.push_inst(Inst::ArrayLoad {
                                dst: old,
                                arr: av,
                                idx: iv,
                            });
                            self.fb.push_inst(Inst::Release { value: old });
                        }
                        self.fb.push_inst(Inst::ArrayStore { arr: av, idx: iv, value: vv });
                    }
                    MirTy::Map { .. } => {
                        self.fb.push_inst(Inst::MapSet { map: av, key: iv, value: vv });
                        // Map takes its own +1 share via host_map_set's
                        // retain_by_kind. For a fresh value the caller
                        // also has a transient +1 from the source
                        // expression — release it here so the only
                        // remaining share is the map's. Borrowed values
                        // (use_local etc.) leave their slot's share to
                        // be dropped by scope-exit release as usual.
                        if value_is_fresh && vty.is_heap() {
                            self.fb.push_inst(Inst::Release { value: vv });
                        }
                    }
                    other => {
                        return Err(LowerError::Other(format!(
                            "index assignment on non-array/map: {other}"
                        )))
                    }
                }
                Ok((self.const_unit(), MirTy::Unit))
            }
            ExprKind::Field { obj, name } => self.lower_field(obj, *name, expr.span),
            ExprKind::ForIn { var, iter, body } => self.lower_for_in(*var, iter, body),
            ExprKind::IfLet { name, expr: scrut, then_branch, else_branch } => {
                self.lower_if_let(*name, scrut, then_branch, else_branch.as_deref())
            }
            ExprKind::Range { .. } => Err(LowerError::Other(
                "range only valid as a for-in iter (rejected by the type checker elsewhere)".into(),
            )),
            ExprKind::MethodCall { obj, method, args } => {
                self.lower_method_call(obj, *method, args, expr.span)
            }
            ExprKind::EnumCtor { enum_name, variant, args } => {
                self.lower_enum_ctor(*enum_name, *variant, args)
            }
            ExprKind::FnExpr { params, ret, body } => {
                self.lower_fn_expr(params, ret.as_ref(), body, expr.span)
            }
            ExprKind::Closure { .. } => Err(LowerError::Other(
                "ExprKind::Closure encountered (legacy hoist form, unused)".into(),
            )),
            ExprKind::Match { scrutinee, arms } => self.lower_match(scrutinee, arms),
            ExprKind::MapLit(entries) => self.lower_map_literal(entries),
            ExprKind::StructLit { class, fields } => {
                // Aggregate literal for an @extern(C) struct. Desugars
                // to `new C()` (zero-init) + field stores.
                let class_id = self
                    .class_meta
                    .iter()
                    .find_map(|(cid, _)| {
                        if self.classes[cid.0 as usize].name == *class {
                            Some(*cid)
                        } else {
                            None
                        }
                    })
                    .ok_or_else(|| LowerError::Other(format!("unknown class {class}")))?;
                let dst = self.fb.new_value(MirTy::Object(class_id));
                self.fb.push_inst(Inst::NewObject {
                    dst,
                    class: class_id,
                    init_args: Box::new([]),
                    init: FuncId(u32::MAX),
                });
                for (fname, fval) in fields.iter() {
                    let meta = self.class_meta.get(&class_id).unwrap();
                    let fid = *meta.field_ix.get(fname).ok_or_else(|| {
                        LowerError::Other(format!("no field {fname} on {class}"))
                    })?;
                    let fty = meta.field_ty.get(&fid).cloned().unwrap();
                    let (vv, vty) = self.lower_expr(fval)?;
                    let coerced = if vty == fty {
                        vv
                    } else {
                        self.coerce(vv, &vty, &fty, fval.span)?
                    };
                    self.fb.push_inst(Inst::StoreField { obj: dst, field: fid, value: coerced });
                }
                Ok((dst, MirTy::Object(class_id)))
            }
            // M1 is feature-complete — every variant of `ExprKind`
            // is handled above. Removed the legacy catch-all because
            // the compiler now flags it as `unreachable_pattern`.
        }
    }

    fn lower_map_literal(
        &mut self,
        entries: &[(Expr, Expr)],
    ) -> Result<(ValueId, MirTy), LowerError> {
        if entries.is_empty() {
            // Empty map literal isn't valid surface syntax (`{}`
            // parses as a block); emit a fallback Map<string, i64>
            // and let the binding annotation override.
            let key = MirTy::Str;
            let val = MirTy::I64;
            let ty = MirTy::Map {
                key: Box::new(key.clone()),
                val: Box::new(val.clone()),
            };
            let v = self.fb.new_value(ty.clone());
            self.fb.push_inst(Inst::NewMap {
                dst: v,
                key,
                val,
                entries: Box::new([]),
            });
            return Ok((v, ty));
        }
        let mut pairs = Vec::with_capacity(entries.len());
        let mut key_ty: Option<MirTy> = None;
        let mut val_ty: Option<MirTy> = None;
        for (k, v) in entries {
            let (kv, kty) = self.lower_expr(k)?;
            let (vv, vty) = self.lower_expr(v)?;
            let ek = key_ty.get_or_insert(kty.clone()).clone();
            let ev = val_ty.get_or_insert(vty.clone()).clone();
            let kv = if kty == ek {
                kv
            } else {
                self.coerce(kv, &kty, &ek, k.span)?
            };
            let vv = if vty == ev {
                vv
            } else {
                self.coerce(vv, &vty, &ev, v.span)?
            };
            pairs.push((kv, vv));
        }
        let key = key_ty.unwrap();
        let val = val_ty.unwrap();
        let ty = MirTy::Map {
            key: Box::new(key.clone()),
            val: Box::new(val.clone()),
        };
        let dst = self.fb.new_value(ty.clone());
        self.fb.push_inst(Inst::NewMap {
            dst,
            key,
            val,
            entries: pairs.into_boxed_slice(),
        });
        Ok((dst, ty))
    }

    fn lower_array_literal_with_hint(
        &mut self,
        items: &[Expr],
        elem_hint: Option<MirTy>,
        len_hint: Option<usize>,
    ) -> Result<(ValueId, MirTy), LowerError> {
        if items.is_empty() {
            let elem = elem_hint.unwrap_or(MirTy::I64);
            let ty = MirTy::Array { elem: Box::new(elem.clone()), len: len_hint };
            let v = self.fb.new_value(ty.clone());
            self.fb.push_inst(Inst::NewArrayEmpty {
                dst: v,
                elem,
                fixed_len: len_hint,
            });
            return Ok((v, ty));
        }
        let mut elem_vals = Vec::with_capacity(items.len());
        let mut elem_ty: Option<MirTy> = elem_hint.clone();
        for it in items {
            let elem_is_fresh = self.is_fresh_object_expr(it);
            let (vv, vty) = self.lower_expr(it)?;
            let target = elem_ty.get_or_insert(vty.clone()).clone();
            let coerced = if target == vty {
                vv
            } else {
                self.coerce(vv, &vty, &target, it.span)?
            };
            // Mirror the no-hint path: aliased heap elements need
            // a +1 because host_release_array cascade-releases each
            // stored Object on drop.
            let is_heap = matches!(
                target,
                MirTy::Object(_)
                    | MirTy::Array { .. }
                    | MirTy::Tuple(_)
                    | MirTy::Map { .. }
                    | MirTy::Optional(_)
                    | MirTy::Fn(_)
                    | MirTy::Str
            );
            if is_heap && !elem_is_fresh {
                self.fb.push_inst(Inst::Retain { value: coerced });
            }
            elem_vals.push(coerced);
        }
        let elem = elem_ty.unwrap();
        let ty = MirTy::Array { elem: Box::new(elem.clone()), len: len_hint };
        let v = self.fb.new_value(ty.clone());
        self.fb.push_inst(Inst::NewArray {
            dst: v,
            elem,
            items: elem_vals.into_boxed_slice(),
        });
        Ok((v, ty))
    }

    fn lower_array_literal(&mut self, items: &[Expr]) -> Result<(ValueId, MirTy), LowerError> {
        if items.is_empty() {
            // `[]` requires a type annotation; the let stmt's coerce
            // step would correct the element type. Fall back to i64
            // here; this is rare enough that letting it be obviously
            // wrong is fine for now (the binding's type annotation
            // path is the supported way).
            let ty = MirTy::Array { elem: Box::new(MirTy::I64), len: None };
            let v = self.fb.new_value(ty.clone());
            self.fb.push_inst(Inst::NewArrayEmpty {
                dst: v,
                elem: MirTy::I64,
                fixed_len: None,
            });
            return Ok((v, ty));
        }
        let mut elem_vals = Vec::with_capacity(items.len());
        let mut elem_ty: Option<MirTy> = None;
        for it in items {
            let elem_is_fresh = self.is_fresh_object_expr(it);
            let (vv, vty) = self.lower_expr(it)?;
            let ty = elem_ty.get_or_insert(vty.clone()).clone();
            let coerced = if ty == vty {
                vv
            } else {
                self.coerce(vv, &vty, &ty, it.span)?
            };
            // Array elements: each slot owns +1 because the array's
            // host_release_array cascade calls release_object on
            // every stored Object on drop. Fresh values already
            // come with +1 (transfer); aliased Vars don't, so we
            // bump rc here. Without this, `let xs = [a, a]` plus
            // the eventual array drop double-frees `a`.
            let is_heap = matches!(
                ty,
                MirTy::Object(_)
                    | MirTy::Array { .. }
                    | MirTy::Tuple(_)
                    | MirTy::Map { .. }
                    | MirTy::Optional(_)
                    | MirTy::Fn(_)
                    | MirTy::Str
            );
            if is_heap && !elem_is_fresh {
                self.fb.push_inst(Inst::Retain { value: coerced });
            }
            elem_vals.push(coerced);
        }
        let elem = elem_ty.unwrap();
        let ty = MirTy::Array { elem: Box::new(elem.clone()), len: None };
        let v = self.fb.new_value(ty.clone());
        self.fb.push_inst(Inst::NewArray {
            dst: v,
            elem,
            items: elem_vals.into_boxed_slice(),
        });
        Ok((v, ty))
    }

    fn lower_tuple_literal(&mut self, items: &[Expr]) -> Result<(ValueId, MirTy), LowerError> {
        let mut vals = Vec::with_capacity(items.len());
        let mut tys = Vec::with_capacity(items.len());
        for it in items {
            let elem_is_fresh = self.is_fresh_object_expr(it);
            let (v, t) = self.lower_expr(it)?;
            // Tuple slots own their stored heap value's +1, mirroring
            // the array-literal element-retain rule. Without this,
            // `(read, bump)` over locals like `let read = fn(){...}`
            // would let the surrounding scope-exit release the
            // closure to rc=0 and free it while the tuple still
            // points there.
            let is_heap = matches!(
                t,
                MirTy::Object(_)
                    | MirTy::Array { .. }
                    | MirTy::Tuple(_)
                    | MirTy::Map { .. }
                    | MirTy::Optional(_)
                    | MirTy::Fn(_)
                    | MirTy::Str
            );
            if is_heap && !elem_is_fresh {
                self.fb.push_inst(Inst::Retain { value: v });
            }
            vals.push(v);
            tys.push(t);
        }
        let ty = MirTy::Tuple(tys.into_boxed_slice());
        let v = self.fb.new_value(ty.clone());
        self.fb.push_inst(Inst::NewTuple {
            dst: v,
            items: vals.into_boxed_slice(),
        });
        Ok((v, ty))
    }

    fn lower_index(&mut self, obj: &Expr, index: &Expr) -> Result<(ValueId, MirTy), LowerError> {
        let obj_is_fresh = self.is_fresh_object_expr(obj);
        let (av, aty) = self.lower_expr(obj)?;
        match &aty {
            MirTy::Array { elem, .. } => {
                let elem_ty = (**elem).clone();
                let (iv, _) = self.lower_expr(index)?;
                let v = self.fb.new_value(elem_ty.clone());
                self.fb.push_inst(Inst::ArrayLoad { dst: v, arr: av, idx: iv });
                // Fresh-array index: retain the selected element so
                // the array's own Release (cascading deinit on every
                // stored Object) doesn't drop it. The unselected
                // elements get their deinits via the cascade.
                if obj_is_fresh && matches!(elem_ty, MirTy::Object(_)) {
                    self.fb.push_inst(Inst::Retain { value: v });
                    self.fb.push_inst(Inst::Release { value: av });
                }
                Ok((v, elem_ty))
            }
            MirTy::Map { val, .. } => {
                let val_ty = (**val).clone();
                let (kv, _) = self.lower_expr(index)?;
                let v = self.fb.new_value(val_ty.clone());
                self.fb.push_inst(Inst::MapGet { dst: v, map: av, key: kv });
                if obj_is_fresh && matches!(val_ty, MirTy::Object(_)) {
                    self.fb.push_inst(Inst::Retain { value: v });
                    self.fb.push_inst(Inst::Release { value: av });
                }
                Ok((v, val_ty))
            }
            MirTy::Tuple(elems) => {
                let idx = match &index.kind {
                    ExprKind::Int(n) if *n >= 0 => *n as u32,
                    _ => {
                        return Err(LowerError::Other(
                            "tuple index must be a non-negative integer literal".into(),
                        ))
                    }
                };
                let elem_ty = elems
                    .get(idx as usize)
                    .cloned()
                    .ok_or_else(|| LowerError::Other(format!("tuple index {idx} out of range")))?;
                let v = self.fb.new_value(elem_ty.clone());
                self.fb.push_inst(Inst::TupleExtract { dst: v, tup: av, idx });
                // Fresh-tuple-on-index cleanup: extract may keep one
                // element alive (the selected one), but the others are
                // about to leak. Retain the selected Object so it
                // outlives the per-element release sweep, then release
                // every Object element of the fresh tuple.
                if obj_is_fresh {
                    if matches!(elem_ty, MirTy::Object(_)) {
                        self.fb.push_inst(Inst::Retain { value: v });
                    }
                    for (i, ety) in elems.iter().enumerate() {
                        if matches!(ety, MirTy::Object(_)) {
                            let ev = self.fb.new_value(ety.clone());
                            self.fb.push_inst(Inst::TupleExtract {
                                dst: ev,
                                tup: av,
                                idx: i as u32,
                            });
                            self.fb.push_inst(Inst::Release { value: ev });
                        }
                    }
                }
                Ok((v, elem_ty))
            }
            other => Err(LowerError::Other(format!("indexing non-indexable type {other}"))),
        }
    }

    fn lower_field(
        &mut self,
        obj: &Expr,
        name: Symbol,
        _span: Span,
    ) -> Result<(ValueId, MirTy), LowerError> {
        // `typeof(x).name` — pseudo-property on the Type handle that
        // `typeof` returns. Lower obj (yields the dynamic class id),
        // then call `class_name` builtin to get the class name.
        if name.as_str() == "name" {
            if let ExprKind::Call { callee, args } = &obj.kind {
                if callee.as_str() == "typeof" && args.len() == 1 {
                    let (cid, _) = self.lower_expr(obj)?;
                    let v = self.fb.new_value(MirTy::Str);
                    self.fb.push_inst(Inst::Call {
                        dst: Some(v),
                        callee: FuncRef::Builtin(Symbol::intern("class_name")),
                        args: Box::new([cid]),
                    });
                    return Ok((v, MirTy::Str));
                }
            }
        }
        // `ClassName.field` — static/const access. The receiver is
        // a bare identifier that names a class, not an instance.
        if let ExprKind::Var(maybe_class) = &obj.kind {
            if self.lookup_var(*maybe_class).is_none() {
                if let Some((cid, _)) = self
                    .class_meta
                    .iter()
                    .find(|(cid, _)| self.classes[cid.0 as usize].name == *maybe_class)
                {
                    let meta = self.class_meta.get(cid).unwrap();
                    if let Some(&slot) = meta.static_slots.get(&name) {
                        let slot_owner = &self.classes[cid.0 as usize];
                        let ty = self
                            .classes[cid.0 as usize]
                            .statics
                            .iter()
                            .find_map(|sid| {
                                let s = &self.statics_by_id(*sid);
                                if s.name == name {
                                    Some(s.ty.clone())
                                } else {
                                    None
                                }
                            })
                            .unwrap_or(MirTy::I64);
                        let _ = slot_owner;
                        let v = self.fb.new_value(ty.clone());
                        self.fb.push_inst(Inst::LoadStatic { dst: v, slot });
                        return Ok((v, ty));
                    }
                }
            }
        }
        let obj_is_fresh = self.is_fresh_object_expr(obj);
        let (ov, oty) = self.lower_expr(obj)?;
        // Property getter on an instance.
        if let MirTy::Object(cid) = &oty {
            let meta = self.class_meta.get(cid).expect("class meta");
            if let Some((mid, prop_ty)) = meta.property_getter.get(&name).cloned() {
                let v = self.fb.new_value(prop_ty.clone());
                self.fb.push_inst(Inst::Call {
                    dst: Some(v),
                    callee: FuncRef::Local(mid),
                    args: Box::new([ov]),
                });
                return Ok((v, prop_ty));
            }
        }
        // Built-in `.length` on arrays / strings.
        if name == "length" {
            match &oty {
                MirTy::Array { .. } => {
                    let v = self.fb.new_value(MirTy::I64);
                    self.fb.push_inst(Inst::ArrayLen { dst: v, arr: ov });
                    return Ok((v, MirTy::I64));
                }
                MirTy::Str => {
                    // String length is a runtime call (Unicode
                    // code-point count). Lower as a builtin.
                    let v = self.fb.new_value(MirTy::I64);
                    self.fb.push_inst(Inst::Call {
                        dst: Some(v),
                        callee: FuncRef::Builtin(Symbol::intern("str_length")),
                        args: Box::new([ov]),
                    });
                    return Ok((v, MirTy::I64));
                }
                _ => {}
            }
        }
        // Optional accessors (.isSome / .isNone).
        if let MirTy::Optional(_) = &oty {
            if name == "isSome" {
                let v = self.fb.new_value(MirTy::Bool);
                self.fb.push_inst(Inst::OptionalIsSome { dst: v, opt: ov });
                return Ok((v, MirTy::Bool));
            }
            if name == "isNone" {
                let s = self.fb.new_value(MirTy::Bool);
                self.fb.push_inst(Inst::OptionalIsSome { dst: s, opt: ov });
                let v = self.fb.new_value(MirTy::Bool);
                self.fb.push_inst(Inst::UnOp { dst: v, op: UnOp::BoolNot, src: s });
                return Ok((v, MirTy::Bool));
            }
        }
        // Class instance field.
        if let MirTy::Object(cid) = &oty {
            let meta = self.class_meta.get(cid).expect("class meta");
            if let Some(&fid) = meta.field_ix.get(&name) {
                let fty = meta.field_ty.get(&fid).cloned().unwrap();
                let v = self.fb.new_value(fty.clone());
                self.fb.push_inst(Inst::LoadField { dst: v, obj: ov, field: fid });
                // Release a fresh-receiver Object after extracting a
                // non-Object field — the receiver is otherwise leaked.
                // Heap-typed fields need a retain first so the
                // cascade triggered by `Release v` doesn't tear the
                // field down: the receiver owned a +1 on the field
                // (the array / map / etc.), and once the receiver's
                // rc hits zero its `__release_object_fields` cascade
                // releases that same +1. Without the retain, the
                // caller gets a dangling pointer.
                if obj_is_fresh && !matches!(fty, MirTy::Object(_)) {
                    retain_if_heap(&mut self.fb, v, &fty);
                    self.fb.push_inst(Inst::Release { value: ov });
                }
                return Ok((v, fty));
            }
            return Err(LowerError::Other(format!(
                "no field `{name}` on class id #{}",
                cid.0
            )));
        }
        Err(LowerError::Other(format!(
            "field `{name}` on unsupported type {oty}"
        )))
    }

    fn lower_fn_expr(
        &mut self,
        params: &[ast::Param],
        ret: Option<&Type>,
        body: &ast::Block,
        span: Span,
    ) -> Result<(ValueId, MirTy), LowerError> {
        // Collect free variables in the FnExpr body.
        let mut bound: std::collections::HashSet<Symbol> =
            params.iter().map(|p| p.name).collect();
        let mut frees: Vec<Symbol> = Vec::new();
        collect_free_vars_block(body, &mut bound, &mut frees);

        // Names that this closure (transitively, through nested
        // FnExprs in its body) writes via `Assign`. These need cell
        // capture so writes persist across calls. Names not in this
        // set are captured by value snapshot — independent per
        // closure (B1 semantics: sibling closures sharing the same
        // outer name do NOT see each other's writes).
        let mut writes: std::collections::HashSet<Symbol> =
            std::collections::HashSet::new();
        collect_mut_assigned_block(body, &mut writes);
        // The closure's own params are local mutable, not captured.
        for p in params.iter() {
            writes.remove(&p.name);
        }

        // Filter out names that aren't bound in the surrounding scope
        // (top-level fns / classes / enums / statics — they're
        // resolved globally, not captured).
        let mut captures: Vec<crate::program::EnvCapture> = Vec::new();
        let mut capture_vals: Vec<ValueId> = Vec::new();
        for name in frees {
            let needs_cell = writes.contains(&name);
            // 1) Source already has a cell binding in current scope —
            // share that cell directly (whether or not we write).
            if let Some((cell_v, inner_ty)) = self.lookup_cell_ptr(name) {
                capture_vals.push(cell_v);
                captures.push(crate::program::EnvCapture {
                    name,
                    ty: inner_ty,
                    is_cell: true,
                });
                continue;
            }
            // 2) Source is a captured cell from the enclosing closure
            // body — load the cell pointer (not its inner value) and
            // forward it.
            if let Some(caps) = self.captures_in_scope {
                if let Some((idx, cty)) = caps.get(&name).cloned() {
                    let outer_is_cell = self
                        .cell_captures
                        .map(|s| s.contains(&name))
                        .unwrap_or(false);
                    if outer_is_cell {
                        let cell_v = self.fb.new_value(MirTy::I64);
                        self.fb.push_inst(Inst::LoadCapture { dst: cell_v, idx });
                        capture_vals.push(cell_v);
                        captures.push(crate::program::EnvCapture {
                            name,
                            ty: cty,
                            is_cell: true,
                        });
                        continue;
                    }
                    // Outer capture is a value snapshot — load it.
                    let v = self.fb.new_value(cty.clone());
                    self.fb.push_inst(Inst::LoadCapture { dst: v, idx });
                    if needs_cell {
                        // Allocate a fresh private cell initialised
                        // from the snapshot. The cell owns its share
                        // of `v`, so retain heap-typed inners before
                        // the store — otherwise the outer scope's
                        // eventual release frees the cell's only
                        // backing object.
                        retain_if_heap(&mut self.fb, v, &cty);
                        let cell_ty = MirTy::Array {
                            elem: Box::new(cty.clone()),
                            len: None,
                        };
                        let cell_v = self.fb.new_value(cell_ty);
                        self.fb.push_inst(Inst::NewArray {
                            dst: cell_v,
                            elem: cty.clone(),
                            items: Box::new([v]),
                        });
                        capture_vals.push(cell_v);
                        captures.push(crate::program::EnvCapture {
                            name,
                            ty: cty,
                            is_cell: true,
                        });
                    } else {
                        capture_vals.push(v);
                        captures.push(crate::program::EnvCapture {
                            name,
                            ty: cty,
                            is_cell: false,
                        });
                    }
                    continue;
                }
            }
            // 3) Source is a regular local / SSA in current scope.
            if let Some((v, ty)) = self.lookup_var(name) {
                if needs_cell {
                    // Allocate a private cell initialised from the
                    // snapshot of the current value. The outer scope
                    // does NOT see writes (sibling-closure isolation).
                    retain_if_heap(&mut self.fb, v, &ty);
                    let cell_ty = MirTy::Array {
                        elem: Box::new(ty.clone()),
                        len: None,
                    };
                    let cell_v = self.fb.new_value(cell_ty);
                    self.fb.push_inst(Inst::NewArray {
                        dst: cell_v,
                        elem: ty.clone(),
                        items: Box::new([v]),
                    });
                    capture_vals.push(cell_v);
                    captures.push(crate::program::EnvCapture {
                        name,
                        ty,
                        is_cell: true,
                    });
                } else {
                    capture_vals.push(v);
                    captures.push(crate::program::EnvCapture {
                        name,
                        ty,
                        is_cell: false,
                    });
                }
                continue;
            }
            // 4) Source is a top-level slot-backed binding. Snapshot
            //    its current value at construction time so the
            //    closure body sees the captured value, not whatever
            //    the slot happens to hold at call time. (Mirrors
            //    standard "capture by value" semantics for fn-expr
            //    free vars.)
            //
            //    Self-recursive closures (`let f = fn(...) { f(...)
            //    }`) are the exception: at construction the slot
            //    hasn't been written yet, so a snapshot would
            //    capture 0/null and a later call would crash.
            //    Detect via `binding_self_name` (set by lower_stmt
            //    while lowering the let value); skip the capture so
            //    the body's `Var` lookup hits the slot fallback at
            //    call time, which is the standard "late binding"
            //    semantics expected for self-reference.
            if Some(name) == self.binding_self_name {
                continue;
            }
            if let Some((idx, slot_ty)) = self.repl_slots.get(&name).cloned() {
                let idx_v = self.const_int(MirTy::I64, idx as i64);
                let raw = self.fb.new_value(MirTy::I64);
                self.fb.push_inst(Inst::Call {
                    dst: Some(raw),
                    callee: FuncRef::Builtin(Symbol::intern("__repl_load_slot")),
                    args: Box::new([idx_v]),
                });
                let v = self.i64_to_slot_value(raw, &slot_ty)?;
                capture_vals.push(v);
                captures.push(crate::program::EnvCapture {
                    name,
                    ty: slot_ty,
                    is_cell: false,
                });
                continue;
            }
            // Names that aren't local and aren't captures from an
            // outer closure are assumed global (top-level fn / class /
            // enum / static); they need no env entry.
        }

        // Allocate a fresh func id and build a placeholder. Resolve
        // param/ret types now so the synthesised fn has a stable sig
        // for any subsequent callers.
        let n = *self.anon_counter;
        *self.anon_counter += 1;
        let name = Symbol::intern(&format!("__anon_fn_{n}"));
        let id = FuncId(self.funcs.len() as u32);
        self.funcs.push(placeholder_function(name));
        self.fn_ids.insert(name, id);

        let param_tys: Vec<(Symbol, MirTy)> = params
            .iter()
            .map(|p| Ok((p.name, self.resolve_ty(&p.ty)?)))
            .collect::<Result<_, LowerError>>()?;
        let ret_ty = match ret {
            Some(t) => self.resolve_ty(t)?,
            None => MirTy::Unit,
        };

        // The runtime fn signature is `(params..., env)` — the env
        // pointer is passed as a hidden last param at the ABI level.
        // For MIR sig purposes we keep the user-visible params.
        let sig_params: Vec<MirTy> = param_tys.iter().map(|(_, t)| t.clone()).collect();
        self.fn_sigs.insert(
            name,
            FnSig {
                params: sig_params,
                ret: ret_ty.clone(),
            },
        );

        // Push to the pending queue — body lowered after the outer
        // fn finishes.
        self.pending.push(PendingClosure {
            func_id: id,
            name,
            params: param_tys,
            ret: ret_ty.clone(),
            captures: captures.clone(),
            body: body.clone(),
            span,
            enclosing_this_class: self.this_class,
        });

        // Emit the MakeClosure instruction.
        let fn_ty = MirTy::Fn(Box::new(crate::types::MirFnTy {
            params: captures
                .iter()
                .map(|c| c.ty.clone())
                .chain(std::iter::empty()) // captures are env, not user-visible params
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            ret: ret_ty.clone(),
        }));
        // For simplicity the displayed Fn type is the fn signature
        // sans env. Captures' types live in the EnvLayout on the
        // synthesised fn (set when lowering its body).
        let fn_ty = match fn_ty {
            // Replace the params slot with the user-visible params.
            MirTy::Fn(ft) => {
                let _ = ft;
                let user_params: Box<[MirTy]> = params
                    .iter()
                    .map(|p| self.resolve_ty(&p.ty))
                    .collect::<Result<Vec<_>, _>>()?
                    .into_boxed_slice();
                MirTy::Fn(Box::new(crate::types::MirFnTy {
                    params: user_params,
                    ret: ret_ty,
                }))
            }
            other => other,
        };
        // Retain every heap-typed capture — the closure shares
        // ownership with the outer scope, so its captures must
        // outlive any scope-exit release of the source binding.
        // Cell captures hold a shared cell pointer (the cell itself
        // is a heap array allocated for the FnExpr) and are
        // refcounted at the cell layer separately.
        for (cv, c) in capture_vals.iter().zip(captures.iter()) {
            if c.ty.is_heap() && !c.is_cell {
                self.fb.push_inst(Inst::Retain { value: *cv });
            }
        }
        let dst = self.fb.new_value(fn_ty.clone());
        self.fb.push_inst(Inst::MakeClosure {
            dst,
            func: id,
            captures: capture_vals.into_boxed_slice(),
        });
        Ok((dst, fn_ty))
    }

    fn lower_super_call(
        &mut self,
        method: Option<Symbol>,
        args: &[Expr],
        _span: Span,
    ) -> Result<(ValueId, MirTy), LowerError> {
        let cid = self
            .this_class
            .ok_or_else(|| LowerError::Other("super outside method".into()))?;
        let parent_id = self.classes[cid.0 as usize]
            .parent
            .ok_or_else(|| LowerError::Other("super in class without parent".into()))?;
        let this_sym = Symbol::intern("this");
        let this_v = if let Some((v, _)) = self.lookup_var(this_sym) {
            v
        } else if let Some(caps) = self.captures_in_scope {
            // Closure body — `this` flows in as a captured slot.
            let (idx, cty) = caps
                .get(&this_sym)
                .cloned()
                .ok_or_else(|| LowerError::Other("super: `this` not captured".into()))?;
            let v = self.fb.new_value(cty);
            self.fb.push_inst(Inst::LoadCapture { dst: v, idx });
            v
        } else {
            return Err(LowerError::Other("super: `this` not in scope".into()));
        };

        let parent_meta = self.class_meta.get(&parent_id).unwrap();
        let target_method = method.unwrap_or(Symbol::intern("init"));
        let mid = *parent_meta.method_ids.get(&target_method).ok_or_else(|| {
            LowerError::Other(format!("parent has no method {target_method}"))
        })?;
        let sig = parent_meta.method_sigs.get(&target_method).cloned().unwrap();

        let mut arg_vals = vec![this_v];
        for (i, a) in args.iter().enumerate() {
            let (v, vty) = self.lower_expr(a)?;
            let coerced = match sig.params.get(i + 1) {
                Some(t) if t != &vty => self.coerce(v, &vty, t, a.span)?,
                _ => v,
            };
            arg_vals.push(coerced);
        }
        let dst = if matches!(sig.ret, MirTy::Unit) {
            None
        } else {
            Some(self.fb.new_value(sig.ret.clone()))
        };
        self.fb.push_inst(Inst::Call {
            dst,
            callee: FuncRef::Local(mid),
            args: arg_vals.into_boxed_slice(),
        });
        Ok((dst.unwrap_or_else(|| self.const_unit()), sig.ret))
    }

    fn lower_new(
        &mut self,
        class: Symbol,
        args: &[Expr],
        init_method: Option<Symbol>,
    ) -> Result<(ValueId, MirTy), LowerError> {
        let class_id = *self
            .class_meta
            .iter()
            .find_map(|(cid, _)| {
                let cl = &self.classes[cid.0 as usize];
                if cl.name == class {
                    Some(cid)
                } else {
                    None
                }
            })
            .ok_or_else(|| LowerError::Other(format!("unknown class {class}")))?;
        let meta = self.class_meta.get(&class_id).expect("class meta");

        // The mangle pass writes the chosen init's mangled name into
        // `init_method` when init is overloaded. Otherwise look up
        // `init` (which exists for non-overloaded inits, and also for
        // the no-init "synthetic" case below).
        let init_lookup = init_method.unwrap_or_else(|| Symbol::intern("init"));
        let init_id = meta.method_ids.get(&init_lookup).copied();
        let init_sig = meta.method_sigs.get(&init_lookup).cloned();

        // Lower constructor args.
        let mut arg_vals = Vec::with_capacity(args.len());
        let mut fresh_obj_args: Vec<ValueId> = Vec::new();
        for (i, a) in args.iter().enumerate() {
            let arg_is_fresh = self.is_fresh_object_expr(a);
            let (v, vty) = self.lower_expr(a)?;
            let final_v = if let Some(sig) = &init_sig {
                if let Some(target) = sig.params.get(i + 1) {
                    if vty == *target {
                        v
                    } else {
                        self.coerce(v, &vty, target, a.span)?
                    }
                } else {
                    v
                }
            } else {
                v
            };
            if arg_is_fresh && matches!(vty, MirTy::Object(_)) {
                fresh_obj_args.push(final_v);
            }
            arg_vals.push(final_v);
        }

        let dst = self.fb.new_value(MirTy::Object(class_id));
        let init = init_id
            // Synthesise a no-op init reference for argument-less
            // construction when the class has none. The MIR→clif
            // step interprets `FuncId(u32::MAX)` as "no user init,
            // just zero-init fields".
            .unwrap_or(FuncId(u32::MAX));
        self.fb.push_inst(Inst::NewObject {
            dst,
            class: class_id,
            init_args: arg_vals.into_boxed_slice(),
            init,
        });
        // Release fresh Object args — the constructor took a borrow
        // and any field-store-side retain has already kept what it
        // needs. The fresh +1 from `new T(...)` would otherwise leak.
        for fv in fresh_obj_args {
            self.fb.push_inst(Inst::Release { value: fv });
        }
        Ok((dst, MirTy::Object(class_id)))
    }

    fn lower_method_call(
        &mut self,
        obj: &Expr,
        method: Symbol,
        args: &[Expr],
        _span: Span,
    ) -> Result<(ValueId, MirTy), LowerError> {
        let obj_is_fresh = self.is_fresh_object_expr(obj);
        let _ = obj_is_fresh;
        // `console.log(...)` is a special-cased variadic builtin.
        if let ExprKind::Var(name) = &obj.kind {
            if name.as_str() == "console" && method.as_str() == "log" {
                let mut arg_vals = Vec::with_capacity(args.len());
                let mut fresh_str_args: Vec<ValueId> = Vec::new();
                for a in args {
                    let arg_is_fresh = self.is_fresh_object_expr(a);
                    let (v, vty) = self.lower_expr(a)?;
                    if arg_is_fresh && matches!(vty, MirTy::Str) {
                        fresh_str_args.push(v);
                    }
                    arg_vals.push(v);
                }
                self.fb.push_inst(Inst::Call {
                    dst: None,
                    callee: FuncRef::Builtin(Symbol::intern("console_log")),
                    args: arg_vals.into_boxed_slice(),
                });
                for fv in fresh_str_args {
                    self.fb.push_inst(Inst::Release { value: fv });
                }
                return Ok((self.const_unit(), MirTy::Unit));
            }
            // `ClassName.staticMethod(args)` when the ident names a
            // class with no local shadow.
            if self.lookup_var(*name).is_none() {
                let class_id = self
                    .class_meta
                    .iter()
                    .find_map(|(cid, _)| {
                        if self.classes[cid.0 as usize].name == *name {
                            Some(*cid)
                        } else {
                            None
                        }
                    });
                if let Some(cid) = class_id {
                    let meta = self.class_meta.get(&cid).unwrap();
                    if let Some(&fid) = meta.static_method_ids.get(&method) {
                        let sig = meta.static_method_sigs.get(&method).cloned().unwrap();
                        let mut arg_vals = Vec::with_capacity(args.len());
                        let mut fresh_args: Vec<ValueId> = Vec::new();
                        for (i, a) in args.iter().enumerate() {
                            let arg_is_fresh = self.is_fresh_object_expr(a);
                            let (v, vty) = self.lower_expr(a)?;
                            let coerced = match sig.params.get(i) {
                                Some(t) if t != &vty => self.coerce(v, &vty, t, a.span)?,
                                _ => v,
                            };
                            if arg_is_fresh && matches!(vty, MirTy::Object(_) | MirTy::Str) {
                                fresh_args.push(coerced);
                            }
                            arg_vals.push(coerced);
                        }
                        let dst = if matches!(sig.ret, MirTy::Unit) {
                            None
                        } else {
                            Some(self.fb.new_value(sig.ret.clone()))
                        };
                        self.fb.push_inst(Inst::Call {
                            dst,
                            callee: FuncRef::Local(fid),
                            args: arg_vals.into_boxed_slice(),
                        });
                        for fv in fresh_args {
                            self.fb.push_inst(Inst::Release { value: fv });
                        }
                        return Ok((dst.unwrap_or_else(|| self.const_unit()), sig.ret));
                    }
                }
            }
        }
        let (ov, oty) = self.lower_expr(obj)?;
        // `.toString()` is available on every numeric / bool / string.
        if method.as_str() == "toString" && args.is_empty() {
            if oty.is_int() || oty.is_float() || matches!(oty, MirTy::Bool | MirTy::Str) {
                let v = self.fb.new_value(MirTy::Str);
                let builtin = match &oty {
                    MirTy::Bool => "bool_to_string",
                    MirTy::Str => "str_to_string",
                    t if t.is_float() => "float_to_string",
                    _ => "int_to_string",
                };
                self.fb.push_inst(Inst::Call {
                    dst: Some(v),
                    callee: FuncRef::Builtin(Symbol::intern(builtin)),
                    args: Box::new([ov]),
                });
                return Ok((v, MirTy::Str));
            }
        }
        // Limited builtin dispatch for arrays / Optional / strings.
        // User-class method dispatch arrives with classes (later step).
        match (&oty, method.as_str()) {
            (MirTy::Optional(_), "unwrap") => {
                if !args.is_empty() {
                    return Err(LowerError::Other("Optional.unwrap takes no args".into()));
                }
                let inner = match &oty {
                    MirTy::Optional(t) => (**t).clone(),
                    _ => unreachable!(),
                };
                let v = self.fb.new_value(inner.clone());
                self.fb.push_inst(Inst::OptionalUnwrap { dst: v, opt: ov });
                // The unwrapped value aliases the Optional cell's
                // `value` slot — same heap pointer. Without a retain,
                // the receiver and the Optional cell's eventual
                // cascade-release would both decrement the same rc,
                // double-freeing the inner. Bump rc on heap-typed
                // inners so the two release sites balance. (Caught by
                // ASan as a UAF in `host_release_optional` while
                // tearing down `Optional<Optional<Str>>`.)
                if matches!(
                    inner,
                    MirTy::Object(_)
                        | MirTy::Array { .. }
                        | MirTy::Tuple(_)
                        | MirTy::Map { .. }
                        | MirTy::Optional(_)
                        | MirTy::Fn(_)
                        | MirTy::Str
                ) {
                    self.fb.push_inst(Inst::Retain { value: v });
                }
                Ok((v, inner))
            }
            (MirTy::Array { elem, .. }, "push") => {
                if args.len() != 1 {
                    return Err(LowerError::Other("Array.push takes 1 arg".into()));
                }
                let elem_ty = (**elem).clone();
                let value_is_fresh = self.is_fresh_object_expr(&args[0]);
                let (av, aty) = self.lower_expr(&args[0])?;
                let coerced = if aty == elem_ty {
                    av
                } else {
                    self.coerce(av, &aty, &elem_ty, args[0].span)?
                };
                // Bump rc on borrowed heap values — `array_push` stores
                // the cell verbatim, but `__release_array`'s cascade
                // will eventually release every stored element. Without
                // this retain, `surviving.push(b)` where `b = arr[i]`
                // would share rc with the source slot, dropping the
                // element to 0 when the source local exits and freeing
                // it out from under the receiving array.
                if !value_is_fresh {
                    retain_if_heap(&mut self.fb, coerced, &elem_ty);
                }
                self.fb.push_inst(Inst::Call {
                    dst: None,
                    callee: FuncRef::Builtin(Symbol::intern("array_push")),
                    args: Box::new([ov, coerced]),
                });
                Ok((self.const_unit(), MirTy::Unit))
            }
            (MirTy::Array { elem, .. }, "pop") => {
                let elem_ty = (**elem).clone();
                let opt_ty = MirTy::Optional(Box::new(elem_ty.clone()));
                let v = self.fb.new_value(opt_ty.clone());
                self.fb.push_inst(Inst::Call {
                    dst: Some(v),
                    callee: FuncRef::Builtin(Symbol::intern("array_pop")),
                    args: Box::new([ov]),
                });
                Ok((v, opt_ty))
            }
            (MirTy::Array { .. }, "indexOf") => {
                if args.len() != 1 {
                    return Err(LowerError::Other("Array.indexOf takes 1 arg".into()));
                }
                let (av, _) = self.lower_expr(&args[0])?;
                let v = self.fb.new_value(MirTy::I64);
                self.fb.push_inst(Inst::Call {
                    dst: Some(v),
                    callee: FuncRef::Builtin(Symbol::intern("array_index_of")),
                    args: Box::new([ov, av]),
                });
                Ok((v, MirTy::I64))
            }
            (MirTy::Array { elem, .. }, "map") => {
                if args.len() != 1 {
                    return Err(LowerError::Other("Array.map takes 1 arg".into()));
                }
                let elem_ty = (**elem).clone();
                let (fv, fty) = self.lower_expr(&args[0])?;
                // Result element type is the closure's return type.
                let ret_ty = if let MirTy::Fn(ft) = &fty {
                    ft.ret.clone()
                } else {
                    elem_ty.clone()
                };
                let arr_ty = MirTy::Array { elem: Box::new(ret_ty.clone()), len: None };
                // Pass the result element's KIND_* tag to host_array_map
                // so the result array's drop cascades correctly. Tags
                // mirror compile.rs's `kind_tag_of`.
                let kind = match &ret_ty {
                    MirTy::Object(_) => 1,
                    MirTy::Array { .. } => 2,
                    MirTy::Optional(_) => 3,
                    MirTy::Tuple(_) => 4,
                    MirTy::Map { .. } => 5,
                    MirTy::Fn(_) => 6,
                    MirTy::Str => 7,
                    _ => 0,
                };
                let kind_v = self.const_int(MirTy::I64, kind);
                let v = self.fb.new_value(arr_ty.clone());
                self.fb.push_inst(Inst::Call {
                    dst: Some(v),
                    callee: FuncRef::Builtin(Symbol::intern("array_map")),
                    args: Box::new([ov, fv, kind_v]),
                });
                Ok((v, arr_ty))
            }
            (MirTy::Array { elem, .. }, "filter") => {
                if args.len() != 1 {
                    return Err(LowerError::Other("Array.filter takes 1 arg".into()));
                }
                let arr_ty = MirTy::Array { elem: elem.clone(), len: None };
                let (fv, _) = self.lower_expr(&args[0])?;
                let v = self.fb.new_value(arr_ty.clone());
                self.fb.push_inst(Inst::Call {
                    dst: Some(v),
                    callee: FuncRef::Builtin(Symbol::intern("array_filter")),
                    args: Box::new([ov, fv]),
                });
                Ok((v, arr_ty))
            }
            (MirTy::Array { .. }, "forEach") => {
                if args.len() != 1 {
                    return Err(LowerError::Other("Array.forEach takes 1 arg".into()));
                }
                let (fv, _) = self.lower_expr(&args[0])?;
                self.fb.push_inst(Inst::Call {
                    dst: None,
                    callee: FuncRef::Builtin(Symbol::intern("array_for_each")),
                    args: Box::new([ov, fv]),
                });
                Ok((self.const_unit(), MirTy::Unit))
            }
            (MirTy::Array { elem, .. }, "slice") => {
                let arr_ty = MirTy::Array { elem: elem.clone(), len: None };
                let mut arg_vals = vec![ov];
                for a in args {
                    let (v, _) = self.lower_expr(a)?;
                    arg_vals.push(v);
                }
                let v = self.fb.new_value(arr_ty.clone());
                self.fb.push_inst(Inst::Call {
                    dst: Some(v),
                    callee: FuncRef::Builtin(Symbol::intern("array_slice")),
                    args: arg_vals.into_boxed_slice(),
                });
                Ok((v, arr_ty))
            }
            (MirTy::Array { .. }, "includes") => {
                if args.len() != 1 {
                    return Err(LowerError::Other("Array.includes takes 1 arg".into()));
                }
                let (av, _) = self.lower_expr(&args[0])?;
                let v = self.fb.new_value(MirTy::Bool);
                self.fb.push_inst(Inst::Call {
                    dst: Some(v),
                    callee: FuncRef::Builtin(Symbol::intern("array_includes")),
                    args: Box::new([ov, av]),
                });
                Ok((v, MirTy::Bool))
            }
            (MirTy::Str, m) => {
                let (builtin_name, ret_ty) = match m {
                    "charAt" => ("str_char_at", MirTy::Str),
                    "includes" => ("str_includes", MirTy::Bool),
                    "startsWith" => ("str_starts_with", MirTy::Bool),
                    "endsWith" => ("str_ends_with", MirTy::Bool),
                    "toUpper" => ("str_to_upper", MirTy::Str),
                    "toLower" => ("str_to_lower", MirTy::Str),
                    "trim" => ("str_trim", MirTy::Str),
                    "split" => (
                        "str_split",
                        MirTy::Array { elem: Box::new(MirTy::Str), len: None },
                    ),
                    "replace" => ("str_replace", MirTy::Str),
                    "slice" => ("str_slice", MirTy::Str),
                    other => {
                        return Err(LowerError::Other(format!(
                            "unknown string method `{other}`"
                        )))
                    }
                };
                let mut arg_vals = vec![ov];
                for a in args {
                    let (v, _) = self.lower_expr(a)?;
                    arg_vals.push(v);
                }
                let dst = if matches!(ret_ty, MirTy::Unit) {
                    None
                } else {
                    Some(self.fb.new_value(ret_ty.clone()))
                };
                self.fb.push_inst(Inst::Call {
                    dst,
                    callee: FuncRef::Builtin(Symbol::intern(builtin_name)),
                    args: arg_vals.into_boxed_slice(),
                });
                Ok((dst.unwrap_or_else(|| self.const_unit()), ret_ty))
            }
            (MirTy::Map { key, val }, m) => {
                let (builtin_name, ret_ty) = match m {
                    "get" => (
                        "map_get_optional",
                        MirTy::Optional(Box::new((**val).clone())),
                    ),
                    "has" => ("map_has", MirTy::Bool),
                    "delete" => ("map_delete", MirTy::Bool),
                    "set" => ("map_set", MirTy::Unit),
                    "size" => ("map_size", MirTy::I64),
                    "keys" => (
                        "map_keys",
                        MirTy::Array { elem: Box::new((**key).clone()), len: None },
                    ),
                    "values" => (
                        "map_values",
                        MirTy::Array { elem: Box::new((**val).clone()), len: None },
                    ),
                    other => {
                        return Err(LowerError::Other(format!("unknown map method `{other}`")))
                    }
                };
                let mut arg_vals = vec![ov];
                let mut arg_meta: Vec<(bool, crate::inst::ValueId, MirTy)> = Vec::new();
                for a in args {
                    let arg_is_fresh = self.is_fresh_object_expr(a);
                    let (v, vty) = self.lower_expr(a)?;
                    // Map host fns are uniformly (i64, i64, i64). Cast
                    // smaller / float / bool args to i64 cells.
                    let v_ext = if matches!(vty, MirTy::I64 | MirTy::U64)
                        || vty.is_heap()
                        || vty.is_float()
                    {
                        // i64-shaped or f64-shaped values pass through;
                        // floats reinterpret bits via host
                        // `extend_to_i64` at the codegen layer.
                        v
                    } else if vty.is_int() || matches!(vty, MirTy::Bool) {
                        let dst_v = self.fb.new_value(MirTy::I64);
                        self.fb.push_inst(Inst::Cast {
                            dst: dst_v,
                            kind: crate::inst::CastKind::IntResize,
                            src: v,
                        });
                        dst_v
                    } else {
                        v
                    };
                    arg_vals.push(v_ext);
                    arg_meta.push((arg_is_fresh, v_ext, vty));
                }
                let dst = if matches!(ret_ty, MirTy::Unit) {
                    None
                } else {
                    Some(self.fb.new_value(ret_ty.clone()))
                };
                self.fb.push_inst(Inst::Call {
                    dst,
                    callee: FuncRef::Builtin(Symbol::intern(builtin_name)),
                    args: arg_vals.into_boxed_slice(),
                });
                // m.set takes its own +1 share via host_map_set's
                // retain_by_kind. Mirror the AssignIndex path — for a
                // fresh value the caller's transient +1 is released
                // here so the only remaining share is the map's.
                if m == "set" {
                    if let Some((is_fresh, vv, vty)) = arg_meta.get(1) {
                        if *is_fresh && vty.is_heap() {
                            self.fb.push_inst(Inst::Release { value: *vv });
                        }
                    }
                }
                // Fresh map receiver, non-Object result: release the
                // map after the dispatch so its cascade fires.
                if obj_is_fresh
                    && !matches!(ret_ty, MirTy::Object(_))
                    && m != "get"
                    && m != "set"
                {
                    self.fb.push_inst(Inst::Release { value: ov });
                }
                Ok((dst.unwrap_or_else(|| self.const_unit()), ret_ty))
            }
            (MirTy::Weak(class_id), "get") => {
                let opt_ty = MirTy::Optional(Box::new(MirTy::Object(*class_id)));
                let dst = self.fb.new_value(opt_ty.clone());
                self.fb.push_inst(Inst::WeakUpgrade { dst, weak: ov });
                Ok((dst, opt_ty))
            }
            (MirTy::Object(class_id), _) => {
                let meta = self.class_meta.get(class_id).expect("class meta");
                let mid = *meta.method_ids.get(&method).ok_or_else(|| {
                    LowerError::Other(format!("no method `{method}` on class"))
                })?;
                let sig = meta.method_sigs.get(&method).cloned().unwrap();
                let slot = self.classes[class_id.0 as usize]
                    .methods
                    .iter()
                    .find(|m| m.name == method)
                    .and_then(|m| m.slot);

                let mut arg_vals_all = Vec::with_capacity(args.len() + 1);
                arg_vals_all.push(ov);
                let mut fresh_obj_args: Vec<ValueId> = Vec::new();
                for (i, a) in args.iter().enumerate() {
                    let arg_is_fresh = self.is_fresh_object_expr(a);
                    let (v, vty) = self.lower_expr(a)?;
                    let target = sig.params.get(i + 1);
                    let coerced = match target {
                        Some(t) if t != &vty => self.coerce(v, &vty, t, a.span)?,
                        _ => v,
                    };
                    if arg_is_fresh && matches!(vty, MirTy::Object(_)) {
                        fresh_obj_args.push(coerced);
                    }
                    arg_vals_all.push(coerced);
                }
                let dst = if matches!(sig.ret, MirTy::Unit) {
                    None
                } else {
                    Some(self.fb.new_value(sig.ret.clone()))
                };
                if let Some(slot) = slot {
                    let user_args: Box<[ValueId]> =
                        arg_vals_all[1..].to_vec().into_boxed_slice();
                    self.fb.push_inst(Inst::VirtCall {
                        dst,
                        recv: ov,
                        slot,
                        args: user_args,
                    });
                } else {
                    self.fb.push_inst(Inst::Call {
                        dst,
                        callee: FuncRef::Local(mid),
                        args: arg_vals_all.into_boxed_slice(),
                    });
                }
                for fv in fresh_obj_args {
                    self.fb.push_inst(Inst::Release { value: fv });
                }
                // Release a fresh receiver that nothing else owns, but
                // only when the result isn't itself an Object that may
                // alias the receiver's fields.
                if obj_is_fresh && !matches!(sig.ret, MirTy::Object(_)) {
                    self.fb.push_inst(Inst::Release { value: ov });
                }
                Ok((dst.unwrap_or_else(|| self.const_unit()), sig.ret))
            }
            _ => Err(LowerError::Unsupported(
                "method call on this type / unhandled builtin",
            )),
        }
    }

    fn lower_for_in(
        &mut self,
        var: Symbol,
        iter: &Expr,
        body: &AstBlock,
    ) -> Result<(ValueId, MirTy), LowerError> {
        // `for x in <iter> { body }` desugars to a counter loop.
        // Three iter shapes:
        //   - bounded range start..end (or start..=end)
        //   - open range start..       (no upper bound; body must break)
        //   - array
        match &iter.kind {
            ExprKind::Range { start, end, inclusive } => {
                let start = start.as_deref().ok_or_else(|| {
                    LowerError::Other("range without lower bound is not iterable".into())
                })?;
                let (sv, sty) = self.lower_expr(start)?;
                if !sty.is_int() {
                    return Err(LowerError::Other("range bounds must be integer".into()));
                }
                let header = self.fb.new_block();
                let body_blk = self.fb.new_block();
                let exit = self.fb.new_block();
                let i = self.fb.add_block_param(header, sty.clone());

                self.fb.set_terminator(Terminator::Br {
                    dst: header,
                    args: Box::new([sv]),
                });
                self.fb.switch_to(header);

                let cond = if let Some(e) = end {
                    let (ev, _) = self.lower_expr(e)?;
                    let cond_op = if *inclusive {
                        cmp_op(&sty, Cmp::Le)
                    } else {
                        cmp_op(&sty, Cmp::Lt)
                    };
                    let c = self.fb.new_value(MirTy::Bool);
                    self.fb.push_inst(Inst::BinOp {
                        dst: c,
                        op: cond_op,
                        lhs: i,
                        rhs: ev,
                    });
                    Some(c)
                } else {
                    None
                };

                if let Some(c) = cond {
                    self.fb.set_terminator(Terminator::CondBr {
                        cond: c,
                        then_block: body_blk,
                        then_args: Box::new([]),
                        else_block: exit,
                        else_args: Box::new([]),
                    });
                } else {
                    self.fb.set_terminator(Terminator::Br { dst: body_blk, args: Box::new([]) });
                }

                // Step block: increments `i` and back-edges to header.
                // `continue` targets this so the increment isn't
                // skipped.
                let step = self.fb.new_block();

                self.fb.switch_to(body_blk);
                self.env.enter_scope();
                self.env.bind(var, i, sty.clone());
                self.loops.push(LoopFrame {
                    env_depth_at_entry: self.env.scopes.len(),
                    continue_target: step,
                    break_target: exit,
                });
                let _ = self.lower_block(body)?;
                self.loops.pop();
                self.env.exit_scope();
                self.fb.set_terminator(Terminator::Br { dst: step, args: Box::new([]) });

                self.fb.switch_to(step);
                let one = self.const_int(sty.clone(), 1);
                let next = self.fb.new_value(sty.clone());
                self.fb.push_inst(Inst::BinOp {
                    dst: next,
                    op: BinOp::IAdd,
                    lhs: i,
                    rhs: one,
                });
                self.fb.set_terminator(Terminator::Br {
                    dst: header,
                    args: Box::new([next]),
                });

                self.fb.switch_to(exit);
                Ok((self.const_unit(), MirTy::Unit))
            }
            _ => {
                let iter_is_fresh = self.is_fresh_object_expr(iter);
                let (av, aty) = self.lower_expr(iter)?;
                let elem_ty = match &aty {
                    MirTy::Array { elem, .. } => (**elem).clone(),
                    other => {
                        return Err(LowerError::Other(format!(
                            "for-in over non-array / non-range: {other}"
                        )))
                    }
                };
                let len = self.fb.new_value(MirTy::I64);
                self.fb.push_inst(Inst::ArrayLen { dst: len, arr: av });

                let header = self.fb.new_block();
                let body_blk = self.fb.new_block();
                let exit = self.fb.new_block();
                let i = self.fb.add_block_param(header, MirTy::I64);

                let zero = self.const_int(MirTy::I64, 0);
                self.fb.set_terminator(Terminator::Br {
                    dst: header,
                    args: Box::new([zero]),
                });
                self.fb.switch_to(header);
                let c = self.fb.new_value(MirTy::Bool);
                self.fb.push_inst(Inst::BinOp {
                    dst: c,
                    op: BinOp::ILtS,
                    lhs: i,
                    rhs: len,
                });
                self.fb.set_terminator(Terminator::CondBr {
                    cond: c,
                    then_block: body_blk,
                    then_args: Box::new([]),
                    else_block: exit,
                    else_args: Box::new([]),
                });

                let step = self.fb.new_block();

                self.fb.switch_to(body_blk);
                let elem_v = self.fb.new_value(elem_ty.clone());
                self.fb.push_inst(Inst::ArrayLoad { dst: elem_v, arr: av, idx: i });
                self.env.enter_scope();
                self.env.bind(var, elem_v, elem_ty.clone());
                self.loops.push(LoopFrame {
                    env_depth_at_entry: self.env.scopes.len(),
                    continue_target: step,
                    break_target: exit,
                });
                let _ = self.lower_block(body)?;
                self.loops.pop();
                self.env.exit_scope();
                self.fb.set_terminator(Terminator::Br { dst: step, args: Box::new([]) });

                self.fb.switch_to(step);
                let one = self.const_int(MirTy::I64, 1);
                let next = self.fb.new_value(MirTy::I64);
                self.fb.push_inst(Inst::BinOp {
                    dst: next,
                    op: BinOp::IAdd,
                    lhs: i,
                    rhs: one,
                });
                self.fb.set_terminator(Terminator::Br {
                    dst: header,
                    args: Box::new([next]),
                });

                self.fb.switch_to(exit);
                // After the for-in finishes, a fresh-receiver array
                // has no surviving owner — release it. host_release_array
                // both cascades release_object on every Object element
                // (when the array's kind_tag == 1) and frees the
                // 48-byte header + data buffer. Without this, the
                // fresh array leaks even when its elements are
                // primitives (e.g. `for x in make_arr(): i64[]`).
                let _ = len;
                if iter_is_fresh {
                    self.fb.push_inst(Inst::Release { value: av });
                }
                Ok((self.const_unit(), MirTy::Unit))
            }
        }
    }

    fn lower_enum_ctor(
        &mut self,
        enum_name: Symbol,
        variant: Symbol,
        args: &ast::CtorArgs,
    ) -> Result<(ValueId, MirTy), LowerError> {
        let id = *self.enum_ids.get(&enum_name).ok_or_else(|| {
            LowerError::Other(format!("unknown enum {enum_name}"))
        })?;
        let meta = self.enum_meta.get(&id).expect("enum meta");
        let vmeta = meta.variants.get(&variant).ok_or_else(|| {
            LowerError::Other(format!("enum {enum_name} has no variant {variant}"))
        })?;
        let vid = vmeta.id;
        let payload_meta = vmeta.payload.clone();

        let payload_vals: Vec<ValueId> = match (&payload_meta, args) {
            (VariantPayloadMeta::Unit, ast::CtorArgs::Unit) => Vec::new(),
            (VariantPayloadMeta::Tuple(tys), ast::CtorArgs::Tuple(arg_exprs)) => {
                if tys.len() != arg_exprs.len() {
                    return Err(LowerError::Other(format!(
                        "{enum_name}.{variant} expects {} args, got {}",
                        tys.len(),
                        arg_exprs.len()
                    )));
                }
                let mut out = Vec::with_capacity(tys.len());
                for (i, ae) in arg_exprs.iter().enumerate() {
                    let arg_is_fresh = self.is_fresh_object_expr(ae);
                    let (v, vty) = self.lower_expr(ae)?;
                    let coerced = if vty == tys[i] {
                        v
                    } else {
                        self.coerce(v, &vty, &tys[i], ae.span)?
                    };
                    // Heap payload from an aliased Var: retain so the
                    // enum value owns its own +1. Required now that
                    // host_release_array actually frees memory at
                    // rc==0 (match_fresh_scrutinee.il regression).
                    let needs_retain = !arg_is_fresh
                        && matches!(
                            tys[i],
                            MirTy::Object(_)
                                | MirTy::Array { .. }
                                | MirTy::Tuple(_)
                                | MirTy::Map { .. }
                                | MirTy::Optional(_)
                                | MirTy::Fn(_)
                                | MirTy::Str
                        );
                    if needs_retain {
                        self.fb.push_inst(Inst::Retain { value: coerced });
                    }
                    out.push(coerced);
                }
                out
            }
            (VariantPayloadMeta::Struct(fields), ast::CtorArgs::Struct(arg_named)) => {
                // Reorder by declaration order.
                let mut out = vec![None; fields.len()];
                for (name, ae) in arg_named.iter() {
                    let (idx, fty) = fields
                        .iter()
                        .enumerate()
                        .find_map(|(i, (fname, fty))| {
                            if fname == name {
                                Some((i, fty.clone()))
                            } else {
                                None
                            }
                        })
                        .ok_or_else(|| {
                            LowerError::Other(format!(
                                "{enum_name}.{variant} has no field {name}"
                            ))
                        })?;
                    let arg_is_fresh = self.is_fresh_object_expr(ae);
                    let (v, vty) = self.lower_expr(ae)?;
                    let coerced = if vty == fty {
                        v
                    } else {
                        self.coerce(v, &vty, &fty, ae.span)?
                    };
                    let needs_retain = !arg_is_fresh
                        && matches!(
                            fty,
                            MirTy::Object(_)
                                | MirTy::Array { .. }
                                | MirTy::Tuple(_)
                                | MirTy::Map { .. }
                                | MirTy::Optional(_)
                                | MirTy::Fn(_)
                                | MirTy::Str
                        );
                    if needs_retain {
                        self.fb.push_inst(Inst::Retain { value: coerced });
                    }
                    out[idx] = Some(coerced);
                }
                out.into_iter()
                    .enumerate()
                    .map(|(i, v)| {
                        v.ok_or_else(|| {
                            LowerError::Other(format!(
                                "missing field for {enum_name}.{variant} at idx {i}"
                            ))
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?
            }
            _ => {
                return Err(LowerError::Other(format!(
                    "{enum_name}.{variant} payload-shape mismatch"
                )))
            }
        };

        let ty = MirTy::Enum(id);
        let dst = self.fb.new_value(ty.clone());
        self.fb.push_inst(Inst::NewEnum {
            dst,
            enum_id: id,
            variant: vid,
            payload: payload_vals.into_boxed_slice(),
        });
        Ok((dst, ty))
    }




    fn lower_call(&mut self, callee: Symbol, args: &[Expr]) -> Result<(ValueId, MirTy), LowerError> {
        // Built-in pseudo-functions handled before generic resolution.
        if callee.as_str() == "typeof" && args.len() == 1 {
            let (v, _) = self.lower_expr(&args[0])?;
            let dst = self.fb.new_value(MirTy::I64);
            self.fb.push_inst(Inst::TypeOf { dst, value: v });
            return Ok((dst, MirTy::I64));
        }
        // arrayFromCArray<T>(p: *const T, n: size_t) — special-case
        // before the generic FFI helper table because we need to
        // peek the actual T off the first arg's MirTy (`*const T`)
        // and pass an explicit elem stride to the host helper. Type
        // monomorphisation already substituted T at the source level.
        if callee.as_str() == "arrayFromCArray" && args.len() == 2 {
            let (pv, pty) = self.lower_expr(&args[0])?;
            let (nv, nty) = self.lower_expr(&args[1])?;
            let elem_ty = match &pty {
                MirTy::RawPtr { inner, .. } => (**inner).clone(),
                _ => MirTy::U8,
            };
            // Coerce length to i64.
            let n_i64 = if matches!(nty, MirTy::I64) {
                nv
            } else {
                self.coerce(nv, &nty, &MirTy::I64, args[1].span)?
            };
            // Coerce ptr to i64 so the host helper sees a uniform
            // address.
            let p_i64 = match &pty {
                MirTy::RawPtr { .. } => {
                    let dst = self.fb.new_value(MirTy::I64);
                    self.fb.push_inst(Inst::Cast {
                        dst,
                        kind: crate::inst::CastKind::PtrIntCast,
                        src: pv,
                    });
                    dst
                }
                _ => pv,
            };
            let stride = match &elem_ty {
                MirTy::I8 | MirTy::U8 | MirTy::CChar | MirTy::Bool => 1,
                MirTy::I16 | MirTy::U16 => 2,
                MirTy::I32 | MirTy::U32 | MirTy::F32 => 4,
                _ => 8,
            };
            let kind_tag = if matches!(elem_ty, MirTy::Object(_) | MirTy::Str) { 1 } else { 0 };
            let stride_v = self.const_int(MirTy::I64, stride);
            let kind_v = self.const_int(MirTy::I64, kind_tag);
            let arr_ty = MirTy::Array { elem: Box::new(elem_ty), len: None };
            let dst = self.fb.new_value(arr_ty.clone());
            self.fb.push_inst(Inst::Call {
                dst: Some(dst),
                callee: FuncRef::Builtin(Symbol::intern("__c_array_to_array")),
                args: Box::new([p_i64, n_i64, stride_v, kind_v]),
            });
            return Ok((dst, arr_ty));
        }
        // `readT(p, off): T` / `writeT(p, off, v)` raw-memory FFI
        // marshalling helpers. Each maps the source name (e.g.
        // `readU64`) to the host symbol (`__read_u64`) and the MIR
        // return type the lowerer should use. The args go through
        // unchanged — the host helper does the offset arithmetic
        // and the right-width primitive load/store.
        let mem_io = match callee.as_str() {
            "readI8" => Some(("__read_i8", MirTy::I8)),
            "readI16" => Some(("__read_i16", MirTy::I16)),
            "readI32" => Some(("__read_i32", MirTy::I32)),
            "readI64" => Some(("__read_i64", MirTy::I64)),
            "readU8" => Some(("__read_u8", MirTy::U8)),
            "readU16" => Some(("__read_u16", MirTy::U16)),
            "readU32" => Some(("__read_u32", MirTy::U32)),
            "readU64" => Some(("__read_u64", MirTy::U64)),
            "readF32" => Some(("__read_f32", MirTy::F32)),
            "readF64" => Some(("__read_f64", MirTy::F64)),
            "writeI8" => Some(("__write_i8", MirTy::Unit)),
            "writeI16" => Some(("__write_i16", MirTy::Unit)),
            "writeI32" => Some(("__write_i32", MirTy::Unit)),
            "writeI64" => Some(("__write_i64", MirTy::Unit)),
            "writeU8" => Some(("__write_u8", MirTy::Unit)),
            "writeU16" => Some(("__write_u16", MirTy::Unit)),
            "writeU32" => Some(("__write_u32", MirTy::Unit)),
            "writeU64" => Some(("__write_u64", MirTy::Unit)),
            "writeF32" => Some(("__write_f32", MirTy::Unit)),
            "writeF64" => Some(("__write_f64", MirTy::Unit)),
            _ => None,
        };
        if let Some((host_sym, ret_ty)) = mem_io {
            let mut arg_vals = Vec::with_capacity(args.len());
            for (i, a) in args.iter().enumerate() {
                let (mut v, vty) = self.lower_expr(a)?;
                // First arg is the pointer (raw or *const T) — coerce
                // to i64 so the host helper sees a uniform address.
                if i == 0 {
                    if matches!(vty, MirTy::RawPtr { .. }) {
                        let dst = self.fb.new_value(MirTy::I64);
                        self.fb.push_inst(Inst::Cast {
                            dst,
                            kind: crate::inst::CastKind::PtrIntCast,
                            src: v,
                        });
                        v = dst;
                    }
                }
                arg_vals.push(v);
            }
            let dst = if matches!(ret_ty, MirTy::Unit) {
                None
            } else {
                Some(self.fb.new_value(ret_ty.clone()))
            };
            self.fb.push_inst(Inst::Call {
                dst,
                callee: FuncRef::Builtin(Symbol::intern(host_sym)),
                args: arg_vals.into_boxed_slice(),
            });
            return Ok((dst.unwrap_or_else(|| self.const_unit()), ret_ty));
        }
        // FFI marshalling helpers (auto-routed to host symbols).
        let ffi_helper = match callee.as_str() {
            "cstrFromString" => Some(MirTy::I64),
            "stringFromCstr" => Some(MirTy::Str),
            "cstrArrayToStrings" => Some(MirTy::Array {
                elem: Box::new(MirTy::Str),
                len: None,
            }),
            "freeCstr" => Some(MirTy::Unit),
            "errnoCheck" => Some(MirTy::Optional(Box::new(MirTy::I32))),
            "errnoCheckI64" => Some(MirTy::Optional(Box::new(MirTy::I64))),
            _ => None,
        };
        if let Some(ret_ty) = ffi_helper {
            let mut arg_vals = Vec::with_capacity(args.len());
            for a in args {
                let (v, _vty) = self.lower_expr(a)?;
                arg_vals.push(v);
            }
            let dst = if matches!(ret_ty, MirTy::Unit) {
                None
            } else {
                Some(self.fb.new_value(ret_ty.clone()))
            };
            self.fb.push_inst(Inst::Call {
                dst,
                callee: FuncRef::Builtin(callee),
                args: arg_vals.into_boxed_slice(),
            });
            return Ok((dst.unwrap_or_else(|| self.const_unit()), ret_ty));
        }
        // Local fn-typed binding → call_indirect. Also picks up
        // closure captures (the body's `f(...)` where `f` was
        // captured from the outer scope) and REPL persistent slots
        // (a fn value bound at top level in a prior chunk).
        let local_or_capture = self.lookup_var(callee).or_else(|| {
            self.captures_in_scope.and_then(|caps| {
                caps.get(&callee).cloned().map(|(idx, cty)| {
                    let v = self.fb.new_value(cty.clone());
                    self.fb.push_inst(Inst::LoadCapture { dst: v, idx });
                    (v, cty)
                })
            })
            .or_else(|| {
                self.repl_slots.get(&callee).cloned().and_then(|(idx, slot_ty)| {
                    if !matches!(slot_ty, MirTy::Fn(_)) {
                        return None;
                    }
                    let idx_v = self.const_int(MirTy::I64, idx as i64);
                    let raw = self.fb.new_value(MirTy::I64);
                    self.fb.push_inst(Inst::Call {
                        dst: Some(raw),
                        callee: FuncRef::Builtin(Symbol::intern("__repl_load_slot")),
                        args: Box::new([idx_v]),
                    });
                    // Borrow from the slot — the slot keeps the
                    // owning ref. No retain here (the call site
                    // doesn't take persistent ownership of the fn
                    // value, it just invokes it).
                    let v = self.i64_to_slot_value(raw, &slot_ty).ok()?;
                    Some((v, slot_ty))
                })
            })
        });
        if let Some((closure_v, closure_ty)) = local_or_capture {
            if let MirTy::Fn(ft) = &closure_ty {
                let sig_params = ft.params.clone();
                let sig_ret = ft.ret.clone();
                let mut arg_vals = Vec::with_capacity(args.len());
                for (i, a) in args.iter().enumerate() {
                    let (v, vty) = self.lower_expr(a)?;
                    let coerced = match sig_params.get(i) {
                        Some(t) if t != &vty => self.coerce(v, &vty, t, a.span)?,
                        _ => v,
                    };
                    arg_vals.push(coerced);
                }
                let dst = if matches!(sig_ret, MirTy::Unit) {
                    None
                } else {
                    Some(self.fb.new_value(sig_ret.clone()))
                };
                self.fb.push_inst(Inst::CallIndirect {
                    dst,
                    callee: closure_v,
                    sig: crate::inst::FnSig {
                        params: sig_params,
                        ret: sig_ret.clone(),
                        variadic: false,
                    },
                    args: arg_vals.into_boxed_slice(),
                });
                return Ok((dst.unwrap_or_else(|| self.const_unit()), sig_ret));
            }
        }
        // Overloaded fn lookup (multiple candidates registered under
        // `callee`). Pick the one whose param types accept every arg.
        if let Some(candidates) = self.overloads_lookup(callee) {
            if candidates.len() > 1 {
                // Lower args once for type inspection.
                let arg_tys: Vec<(ValueId, MirTy, Span)> = args
                    .iter()
                    .map(|a| {
                        let (v, ty) = self.lower_expr(a)?;
                        Ok((v, ty, a.span))
                    })
                    .collect::<Result<_, LowerError>>()?;

                let pick = pick_overload(self.fn_sigs, &candidates, &arg_tys);
                let chosen = match pick {
                    Some(c) => c,
                    None => {
                        return Err(LowerError::Other(format!(
                            "no matching overload for `{callee}`"
                        )))
                    }
                };
                let sig = self.fn_sigs.get(&chosen).cloned().unwrap();
                let id = *self.fn_ids.get(&chosen).unwrap();
                let mut coerced = Vec::with_capacity(arg_tys.len());
                for (i, (v, vty, span)) in arg_tys.into_iter().enumerate() {
                    let target = sig.params.get(i);
                    let cv = match target {
                        Some(t) if t != &vty => self.coerce(v, &vty, t, span)?,
                        _ => v,
                    };
                    coerced.push(cv);
                }
                let dst = if matches!(sig.ret, MirTy::Unit) {
                    None
                } else {
                    Some(self.fb.new_value(sig.ret.clone()))
                };
                self.fb.push_inst(Inst::Call {
                    dst,
                    callee: FuncRef::Local(id),
                    args: coerced.into_boxed_slice(),
                });
                return Ok((dst.unwrap_or_else(|| self.const_unit()), sig.ret));
            }
        }
        // Free function lookup first.
        if let Some(sig) = self.fn_sigs.get(&callee).cloned() {
            let id = *self.fn_ids.get(&callee).unwrap();
            let is_extern = matches!(
                self.funcs[id.0 as usize].kind,
                FunctionKind::Extern { .. }
            );
            let mut arg_vals = Vec::with_capacity(args.len());
            let mut fresh_obj_args: Vec<ValueId> = Vec::new();
            for (i, a) in args.iter().enumerate() {
                let arg_is_fresh = self.is_fresh_object_expr(a);
                let (v, vty) = self.lower_expr(a)?;
                let coerced = if i < sig.params.len() {
                    match sig.params.get(i) {
                        Some(t) if t != &vty => self.coerce(v, &vty, t, a.span)?,
                        _ => v,
                    }
                } else {
                    v
                };
                if arg_is_fresh && matches!(vty, MirTy::Object(_) | MirTy::Str) {
                    fresh_obj_args.push(coerced);
                }
                arg_vals.push(coerced);
            }
            let callee_ref = if is_extern {
                FuncRef::Local(id)
            } else {
                FuncRef::Local(id)
            };
            let dst = if matches!(sig.ret, MirTy::Unit) {
                None
            } else {
                Some(self.fb.new_value(sig.ret.clone()))
            };
            self.fb.push_inst(Inst::Call {
                dst,
                callee: callee_ref,
                args: arg_vals.into_boxed_slice(),
            });
            for fv in fresh_obj_args {
                self.fb.push_inst(Inst::Release { value: fv });
            }
            return Ok((dst.unwrap_or_else(|| self.const_unit()), sig.ret));
        }
        // Implicit `this.<callee>(args)` inside a method body.
        if let Some(cid) = self.this_class {
            let meta = self.class_meta.get(&cid).expect("class meta");
            if let Some(&mid) = meta.method_ids.get(&callee) {
                let sig = meta.method_sigs.get(&callee).cloned().unwrap();
                let (this_v, _) = self.lookup_var(Symbol::intern("this")).unwrap();
                let mut arg_vals = vec![this_v];
                for (i, a) in args.iter().enumerate() {
                    let (v, vty) = self.lower_expr(a)?;
                    let coerced = match sig.params.get(i + 1) {
                        Some(t) if t != &vty => self.coerce(v, &vty, t, a.span)?,
                        _ => v,
                    };
                    arg_vals.push(coerced);
                }
                let dst = if matches!(sig.ret, MirTy::Unit) {
                    None
                } else {
                    Some(self.fb.new_value(sig.ret.clone()))
                };
                self.fb.push_inst(Inst::Call {
                    dst,
                    callee: FuncRef::Local(mid),
                    args: arg_vals.into_boxed_slice(),
                });
                return Ok((dst.unwrap_or_else(|| self.const_unit()), sig.ret));
            }
        }
        Err(LowerError::Other(format!(
            "call to undeclared function: {callee}"
        )))
    }

}
