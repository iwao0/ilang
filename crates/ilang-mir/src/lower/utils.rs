//! Standalone helpers used across the lowerer:
//!
//! - `placeholder_function`: empty stub fn used while the real body
//!   is still being constructed (forward-declares so call sites can
//!   resolve before the body lowering runs).
//! - `retain_if_heap`: emit a `Retain` only for heap-shaped MirTys.
//! - `mangle_suffix` / `mangle_ty_atom` / `pick_overload` /
//!   `score_coerce`: overload-set selection by parameter shape.
//! - `Cmp` / `cmp_op`: pick the right `BinOp` for a comparison given
//!   the operands' MirTy (signed vs unsigned vs float).
//! - `ty_to_mir`: AST `Type` → `MirTy` translation (re-exported).
//! - `MirTyExt`: soft-deprecated extension trait kept for any
//!   out-of-tree refs.

use std::collections::HashMap;

use ilang_ast::{Span, Symbol, Type};

use crate::builder::FunctionBuilder;
use crate::inst::{BinOp, BlockId, Inst, Terminator, ValueId};
use crate::program::{Function, FunctionKind};
use crate::types::MirTy;

use super::{FnSig, LowerError};

pub(super) fn placeholder_function(name: Symbol) -> Function {
    Function {
        name,
        display_name: name,
        params: Box::new([]),
        ret: MirTy::Unit,
        value_tys: Vec::new(),
        value_spans: Vec::new(),
        blocks: vec![crate::program::Block {
            params: Vec::new(),
            insts: Vec::new(),
            term: Terminator::Unreachable,
        }],
        entry: BlockId(0),
        kind: FunctionKind::Local,
        closure_env: None,
        span: None,
        local_tys: Vec::new(),
        c_symbol: None,
        is_optional: false,
        libs: Vec::new(),
        is_variadic: false,
    }
}

pub(super) fn retain_if_heap(fb: &mut FunctionBuilder, v: ValueId, ty: &MirTy) {
    let heap = matches!(
        ty,
        MirTy::Object(_)
            | MirTy::Fn(_)
            | MirTy::Array { .. }
            | MirTy::Optional(_)
            | MirTy::Tuple(_)
            | MirTy::Map { .. }
            | MirTy::Str
            | MirTy::Enum(_)
    );
    if heap {
        fb.push_inst(Inst::Retain { value: v });
    }
}

/// Encode a parameter type list as a name suffix (`__i64_string` etc.)
/// for overload mangling.
pub(super) fn mangle_suffix(params: &[MirTy]) -> String {
    let mut s = String::from("__");
    for (i, t) in params.iter().enumerate() {
        if i > 0 {
            s.push('_');
        }
        s.push_str(&mangle_ty_atom(t));
    }
    s
}

pub(super) fn mangle_ty_atom(t: &MirTy) -> String {
    match t {
        MirTy::I8 => "i8".into(),
        MirTy::I16 => "i16".into(),
        MirTy::I32 => "i32".into(),
        MirTy::I64 => "i64".into(),
        MirTy::U8 => "u8".into(),
        MirTy::U16 => "u16".into(),
        MirTy::U32 => "u32".into(),
        MirTy::U64 => "u64".into(),
        MirTy::F32 => "f32".into(),
        MirTy::F64 => "f64".into(),
        MirTy::Bool => "bool".into(),
        MirTy::Str => "str".into(),
        MirTy::Unit => "unit".into(),
        MirTy::Object(c) => format!("o{}", c.0),
        MirTy::Weak(c) => format!("w{}", c.0),
        MirTy::Enum(e) => format!("e{}", e.0),
        MirTy::Array { elem, .. } => format!("arr_{}", mangle_ty_atom(elem)),
        MirTy::Tuple(es) => {
            let parts: Vec<String> = es.iter().map(mangle_ty_atom).collect();
            format!("tup_{}", parts.join("_"))
        }
        MirTy::Optional(inner) => format!("opt_{}", mangle_ty_atom(inner)),
        MirTy::Map { key, val } => format!("map_{}_{}", mangle_ty_atom(key), mangle_ty_atom(val)),
        MirTy::Promise(inner) => format!("prom_{}", mangle_ty_atom(inner)),
        MirTy::Fn(_) => "fn".into(),
        MirTy::RawPtr { is_const, inner } => {
            let prefix = if *is_const { "pc" } else { "pm" };
            format!("{prefix}_{}", mangle_ty_atom(inner))
        }
        MirTy::CVoid => "void".into(),
        MirTy::CChar => "char".into(),
        MirTy::Size => "sz".into(),
        MirTy::SSize => "ssz".into(),
        MirTy::TypeVar(s) => format!("tv_{s}"),
        MirTy::Simd { elem, lanes } => {
            let p = match elem {
                crate::types::SimdElem::F32 => "f32",
                crate::types::SimdElem::F64 => "f64",
                crate::types::SimdElem::I8 => "i8",
                crate::types::SimdElem::I16 => "i16",
                crate::types::SimdElem::I32 => "i32",
                crate::types::SimdElem::I64 => "i64",
            };
            format!("simd_{p}x{lanes}")
        }
    }
}

/// Best-match overload selection. Returns the chosen mangled name.
/// Scoring follows syntax.md's rule: exact = 0, widening = 1,
/// f32↔f64 = 1, int→float = 2, T→T? = 3, Object→Weak = 4. Lower wins.
/// Ambiguous ties yield None.
pub(super) fn pick_overload(
    fn_sigs: &HashMap<Symbol, FnSig>,
    candidates: &[Symbol],
    args: &[(ValueId, MirTy, Span)],
) -> Option<Symbol> {
    let mut best: Option<(Symbol, u32)> = None;
    let mut tied = false;
    for cand in candidates {
        let sig = match fn_sigs.get(cand) {
            Some(s) => s,
            None => continue,
        };
        if sig.params.len() != args.len() {
            continue;
        }
        let mut score: u32 = 0;
        let mut ok = true;
        for (i, (_, vty, _)) in args.iter().enumerate() {
            let target = &sig.params[i];
            let s = score_coerce(vty, target);
            match s {
                Some(s) => score += s,
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if !ok {
            continue;
        }
        match &best {
            None => best = Some((*cand, score)),
            Some((_, bs)) => {
                if score < *bs {
                    best = Some((*cand, score));
                    tied = false;
                } else if score == *bs {
                    tied = true;
                }
            }
        }
    }
    if tied {
        None
    } else {
        best.map(|(c, _)| c)
    }
}

pub(super) fn score_coerce(from: &MirTy, to: &MirTy) -> Option<u32> {
    if from == to {
        return Some(0);
    }
    if from.is_signed_int() && to.is_signed_int() && to.int_width() >= from.int_width() {
        return Some(1);
    }
    if from.is_unsigned_int() && to.is_unsigned_int() && to.int_width() >= from.int_width() {
        return Some(1);
    }
    if (from == &MirTy::F32 && to == &MirTy::F64) || (from == &MirTy::F64 && to == &MirTy::F32) {
        return Some(1);
    }
    if from.is_int() && to.is_float() {
        return Some(2);
    }
    if let MirTy::Optional(inner) = to {
        if &**inner == from {
            return Some(3);
        }
    }
    if let (MirTy::Object(c1), MirTy::Weak(c2)) = (from, to) {
        if c1 == c2 {
            return Some(4);
        }
    }
    // Subtype object-to-object: free for now (we treat as Some(0) when
    // exact, otherwise let the caller's coerce path handle it).
    if matches!((from, to), (MirTy::Object(_), MirTy::Object(_))) {
        return Some(0);
    }
    None
}

#[derive(Copy, Clone)]
pub(super) enum Cmp {
    Lt,
    Le,
    Gt,
    Ge,
}

pub(super) fn cmp_op(ty: &MirTy, c: Cmp) -> BinOp {
    if ty.is_float() {
        match c {
            Cmp::Lt => BinOp::FLt,
            Cmp::Le => BinOp::FLe,
            Cmp::Gt => BinOp::FGt,
            Cmp::Ge => BinOp::FGe,
        }
    } else if ty.is_signed_int() {
        match c {
            Cmp::Lt => BinOp::ILtS,
            Cmp::Le => BinOp::ILeS,
            Cmp::Gt => BinOp::IGtS,
            Cmp::Ge => BinOp::IGeS,
        }
    } else {
        match c {
            Cmp::Lt => BinOp::ILtU,
            Cmp::Le => BinOp::ILeU,
            Cmp::Gt => BinOp::IGtU,
            Cmp::Ge => BinOp::IGeU,
        }
    }
}

/// Map an AST `Type` to its MIR counterpart. M1 covers the parts of
/// the language already wired through the lowerer; classes / enums /
/// FFI / generics will be added alongside their lowering work.
pub fn ty_to_mir(t: &Type) -> Result<MirTy, LowerError> {
    Ok(match t {
        Type::I8 => MirTy::I8,
        Type::I16 => MirTy::I16,
        Type::I32 => MirTy::I32,
        Type::I64 => MirTy::I64,
        Type::U8 => MirTy::U8,
        Type::U16 => MirTy::U16,
        Type::U32 => MirTy::U32,
        Type::U64 => MirTy::U64,
        Type::F32 => MirTy::F32,
        Type::F64 => MirTy::F64,
        Type::Bool => MirTy::Bool,
        Type::Str => MirTy::Str,
        Type::Unit => MirTy::Unit,
        Type::Size => MirTy::Size,
        Type::SSize => MirTy::SSize,
        Type::CChar => MirTy::CChar,
        Type::CVoid => MirTy::CVoid,
        Type::Any => return Err(LowerError::Unsupported("Type::Any (variadic builtins)")),
        Type::Object(_) => return Err(LowerError::Unsupported("Object type (classes)")),
        Type::Generic(_) => return Err(LowerError::Unsupported("Generic class instantiation")),
        Type::TypeVar(s) => MirTy::TypeVar(*s),
        Type::Fn(_) => return Err(LowerError::Unsupported("fn types")),
        Type::Enum(_) => return Err(LowerError::Unsupported("enum types")),
        Type::Array { elem, fixed } => MirTy::Array {
            elem: Box::new(ty_to_mir(elem)?),
            len: *fixed,
        },
        Type::Tuple(elems) => {
            let mut out = Vec::with_capacity(elems.len());
            for e in elems.iter() {
                out.push(ty_to_mir(e)?);
            }
            MirTy::Tuple(out.into_boxed_slice())
        }
        Type::Optional(inner) => MirTy::Optional(Box::new(ty_to_mir(inner)?)),
        Type::Weak(_) => return Err(LowerError::Unsupported("weak types")),
        Type::RawPtr { is_const, inner } => {
            // Raw pointers are i64 at runtime; the inner type is
            // only carried for source-level diagnostics. Try to
            // resolve, but on failure (e.g. opaque struct names like
            // `Buf` that ty_to_mir can't see) fall back to `*void`.
            let inner_mir = ty_to_mir(inner).unwrap_or(MirTy::CVoid);
            MirTy::RawPtr {
                is_const: *is_const,
                inner: Box::new(inner_mir),
            }
        }
        Type::Simd { elem, lanes } => {
            let mir_elem = match elem {
                ilang_ast::SimdElem::F32 => crate::types::SimdElem::F32,
                ilang_ast::SimdElem::F64 => crate::types::SimdElem::F64,
                ilang_ast::SimdElem::I8 => crate::types::SimdElem::I8,
                ilang_ast::SimdElem::I16 => crate::types::SimdElem::I16,
                ilang_ast::SimdElem::I32 => crate::types::SimdElem::I32,
                ilang_ast::SimdElem::I64 => crate::types::SimdElem::I64,
            };
            MirTy::Simd {
                elem: mir_elem,
                lanes: *lanes,
            }
        }
    })
}

// Helper for MirTy methods that need shared definitions.
// Predates the inherent `MirTy::int_width` etc. on `crate::types`;
// kept as a soft-deprecated trait for any out-of-tree code that
// still refers to it. The compiler reports it as unused because
// every in-tree call now resolves to the inherent method.
#[allow(dead_code)]
pub(super) trait MirTyExt {
    fn is_int(&self) -> bool;
    fn is_signed_int(&self) -> bool;
    fn is_unsigned_int(&self) -> bool;
    fn is_float(&self) -> bool;
    fn is_numeric(&self) -> bool;
    fn int_width(&self) -> u32;
}

impl MirTyExt for MirTy {
    fn is_signed_int(&self) -> bool {
        matches!(
            self,
            MirTy::I8 | MirTy::I16 | MirTy::I32 | MirTy::I64 | MirTy::SSize
        )
    }
    fn is_unsigned_int(&self) -> bool {
        matches!(
            self,
            MirTy::U8 | MirTy::U16 | MirTy::U32 | MirTy::U64 | MirTy::Size
        )
    }
    fn is_int(&self) -> bool {
        self.is_signed_int() || self.is_unsigned_int()
    }
    fn is_float(&self) -> bool {
        matches!(self, MirTy::F32 | MirTy::F64)
    }
    fn is_numeric(&self) -> bool {
        self.is_int() || self.is_float()
    }
    fn int_width(&self) -> u32 {
        match self {
            MirTy::I8 | MirTy::U8 => 8,
            MirTy::I16 | MirTy::U16 => 16,
            MirTy::I32 | MirTy::U32 => 32,
            MirTy::I64 | MirTy::U64 | MirTy::Size | MirTy::SSize => 64,
            _ => 0,
        }
    }
}
