//! Constant folding pass.
//!
//! Folds `BinOp` / `UnOp` / `Cast` instructions whose every operand
//! is a previously-defined `Inst::Const`. The fold turns the
//! instruction into another `Const` carrying the computed value;
//! later uses of the result still flow through the same `ValueId`,
//! so no rename step is needed.
//!
//! Scope is intentionally narrow:
//!
//! * Integer arithmetic / bitwise / comparison on `MirConst::Int`.
//! * Float arithmetic / comparison on `MirConst::F64` (`F32` is
//!   kept as raw bits — folding would need explicit f32 round-trip;
//!   skipped for now).
//! * Boolean negation, logical-NOT on `MirConst::Bool`.
//! * Integer / float negation, bitwise NOT.
//! * `IntResize` casts between integer constants (signed/unsigned
//!   widening is value-preserving; narrowing truncates).
//!
//! Divide-by-zero / integer-overflow patterns are left to the
//! runtime panic path — folding them at compile time would change
//! observable behaviour, so we skip those operand combinations.
//!
//! Iterates to a fixed point so a fold that produces a new constant
//! unlocks further folds in the same pass.

use std::collections::HashMap;

use crate::inst::{BinOp, CastKind, Inst, MirConst, UnOp, ValueId};
use crate::program::{Function, FunctionKind, Program};
use crate::types::MirTy;

#[derive(Debug, Default, Clone, Copy)]
pub struct Stats {
    pub folds_applied: usize,
}

impl std::ops::AddAssign for Stats {
    fn add_assign(&mut self, rhs: Self) {
        self.folds_applied += rhs.folds_applied;
    }
}

const MAX_FUNCTION_ITERS: usize = 5;

pub fn run_program(prog: &mut Program) -> Stats {
    let mut total = Stats::default();
    for f in prog.functions.iter_mut() {
        if matches!(f.kind, FunctionKind::Extern { .. }) {
            continue;
        }
        total += run_function(f);
    }
    total
}

pub fn run_function(func: &mut Function) -> Stats {
    let mut total = Stats::default();
    for _ in 0..MAX_FUNCTION_ITERS {
        let pass = run_function_once(func);
        total += pass;
        if pass.folds_applied == 0 {
            break;
        }
    }
    total
}

fn run_function_once(func: &mut Function) -> Stats {
    // Build a function-wide ValueId → MirConst table from every
    // `Inst::Const`. SSA guarantees each ValueId is defined exactly
    // once, so the single-pass scan is sufficient.
    let mut table: HashMap<ValueId, MirConst> = HashMap::new();
    for block in &func.blocks {
        for inst in &block.insts {
            if let Inst::Const { dst, value } = inst {
                table.insert(*dst, value.clone());
            }
        }
    }
    let mut stats = Stats::default();
    for block in func.blocks.iter_mut() {
        for inst in block.insts.iter_mut() {
            if let Some(folded) = try_fold(inst, &table, &func.value_tys) {
                let dst = inst_dst(inst).expect("foldable inst must have a dst");
                table.insert(dst, folded.clone());
                *inst = Inst::Const { dst, value: folded };
                stats.folds_applied += 1;
            }
        }
    }
    stats
}

fn inst_dst(inst: &Inst) -> Option<ValueId> {
    match inst {
        Inst::BinOp { dst, .. }
        | Inst::UnOp { dst, .. }
        | Inst::Cast { dst, .. } => Some(*dst),
        _ => None,
    }
}

fn try_fold(
    inst: &Inst,
    table: &HashMap<ValueId, MirConst>,
    value_tys: &[MirTy],
) -> Option<MirConst> {
    match inst {
        Inst::BinOp { op, lhs, rhs, .. } => {
            let l = table.get(lhs)?;
            let r = table.get(rhs)?;
            fold_binop(*op, l, r)
        }
        Inst::UnOp { op, src, .. } => {
            let s = table.get(src)?;
            fold_unop(*op, s)
        }
        Inst::Cast { kind: CastKind::IntResize, src, dst } => {
            // Source must be an Int constant; the result type
            // determines whether to mask to a narrower width.
            let s = table.get(src)?;
            let v = match s {
                MirConst::Int(v) => *v,
                _ => return None,
            };
            let dst_ty = value_tys.get(dst.0 as usize)?;
            Some(MirConst::Int(narrow_int(v, dst_ty)))
        }
        _ => None,
    }
}

/// Truncate / sign-extend a 64-bit constant to fit the destination
/// integer type's bit width. Mirrors what the backend would emit
/// for an `IntResize` cast at run time.
fn narrow_int(v: i64, ty: &MirTy) -> i64 {
    match ty {
        MirTy::I8 | MirTy::CChar => (v as i8) as i64,
        MirTy::U8 => (v as u8) as i64,
        MirTy::I16 => (v as i16) as i64,
        MirTy::U16 => (v as u16) as i64,
        MirTy::I32 => (v as i32) as i64,
        MirTy::U32 => (v as u32) as i64,
        // i64 / u64 / size / ssize: no narrowing needed.
        _ => v,
    }
}

fn fold_binop(op: BinOp, l: &MirConst, r: &MirConst) -> Option<MirConst> {
    match (l, r) {
        (MirConst::Int(a), MirConst::Int(b)) => fold_int_binop(op, *a, *b),
        (MirConst::F64(a), MirConst::F64(b)) => {
            let af = f64::from_bits(*a);
            let bf = f64::from_bits(*b);
            fold_f64_binop(op, af, bf)
        }
        (MirConst::Bool(a), MirConst::Bool(b)) => match op {
            BinOp::IAnd => Some(MirConst::Bool(*a && *b)),
            BinOp::IOr => Some(MirConst::Bool(*a || *b)),
            BinOp::IXor => Some(MirConst::Bool(*a ^ *b)),
            BinOp::IEq => Some(MirConst::Bool(a == b)),
            BinOp::INe => Some(MirConst::Bool(a != b)),
            _ => None,
        },
        _ => None,
    }
}

fn fold_int_binop(op: BinOp, a: i64, b: i64) -> Option<MirConst> {
    // Skip ops that could panic / overflow at run time so folding
    // doesn't silently change observable behaviour.
    if matches!(op, BinOp::IDivS | BinOp::IDivU | BinOp::IRemS | BinOp::IRemU) && b == 0 {
        return None;
    }
    if matches!(op, BinOp::IShl | BinOp::IShrS | BinOp::IShrU) && !(0..64).contains(&b) {
        return None;
    }
    let v = match op {
        BinOp::IAdd => a.wrapping_add(b),
        BinOp::ISub => a.wrapping_sub(b),
        BinOp::IMul => a.wrapping_mul(b),
        BinOp::IDivS => a.wrapping_div(b),
        BinOp::IDivU => ((a as u64).wrapping_div(b as u64)) as i64,
        BinOp::IRemS => a.wrapping_rem(b),
        BinOp::IRemU => ((a as u64).wrapping_rem(b as u64)) as i64,
        BinOp::IShl => a.wrapping_shl(b as u32),
        BinOp::IShrS => a.wrapping_shr(b as u32),
        BinOp::IShrU => ((a as u64).wrapping_shr(b as u32)) as i64,
        BinOp::IAnd => a & b,
        BinOp::IOr => a | b,
        BinOp::IXor => a ^ b,
        BinOp::IEq => return Some(MirConst::Bool(a == b)),
        BinOp::INe => return Some(MirConst::Bool(a != b)),
        BinOp::ILtS => return Some(MirConst::Bool(a < b)),
        BinOp::ILeS => return Some(MirConst::Bool(a <= b)),
        BinOp::IGtS => return Some(MirConst::Bool(a > b)),
        BinOp::IGeS => return Some(MirConst::Bool(a >= b)),
        BinOp::ILtU => return Some(MirConst::Bool((a as u64) < (b as u64))),
        BinOp::ILeU => return Some(MirConst::Bool((a as u64) <= (b as u64))),
        BinOp::IGtU => return Some(MirConst::Bool((a as u64) > (b as u64))),
        BinOp::IGeU => return Some(MirConst::Bool((a as u64) >= (b as u64))),
        _ => return None,
    };
    Some(MirConst::Int(v))
}

fn fold_f64_binop(op: BinOp, a: f64, b: f64) -> Option<MirConst> {
    let v = match op {
        BinOp::FAdd => a + b,
        BinOp::FSub => a - b,
        BinOp::FMul => a * b,
        BinOp::FDiv => a / b,
        BinOp::FEq => return Some(MirConst::Bool(a == b)),
        BinOp::FNe => return Some(MirConst::Bool(a != b)),
        BinOp::FLt => return Some(MirConst::Bool(a < b)),
        BinOp::FLe => return Some(MirConst::Bool(a <= b)),
        BinOp::FGt => return Some(MirConst::Bool(a > b)),
        BinOp::FGe => return Some(MirConst::Bool(a >= b)),
        _ => return None,
    };
    Some(MirConst::F64(v.to_bits()))
}

fn fold_unop(op: UnOp, src: &MirConst) -> Option<MirConst> {
    match (op, src) {
        (UnOp::INeg, MirConst::Int(v)) => Some(MirConst::Int(v.wrapping_neg())),
        (UnOp::FNeg, MirConst::F64(bits)) => {
            let f = f64::from_bits(*bits);
            Some(MirConst::F64((-f).to_bits()))
        }
        (UnOp::Not, MirConst::Int(v)) => Some(MirConst::Int(!*v)),
        (UnOp::BoolNot, MirConst::Bool(b)) => Some(MirConst::Bool(!*b)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inst::{BinOp, BlockId, FuncId, Inst, MirConst, Terminator, UnOp, ValueId};
    use crate::program::{Block, FuncParam, Function, FunctionKind, Program};

    fn intern(s: &str) -> ilang_ast::Symbol {
        ilang_ast::Symbol::intern(s)
    }

    /// `let r = 2 + 3 * 4` folds across two BinOps in one fixed-point
    /// pass: the multiply produces a new Const, the add then picks
    /// it up.
    #[test]
    fn folds_arithmetic_chain() {
        // v0 = const 2; v1 = const 3; v2 = const 4
        // v3 = v1 * v2
        // v4 = v0 + v3
        // return v4
        let main = Function {
            name: intern("main"),
            display_name: intern("main"),
            params: Box::new([] as [FuncParam; 0]),
            ret: MirTy::I64,
            value_tys: vec![MirTy::I64; 5],
            value_spans: vec![None; 5],
            blocks: vec![Block {
                params: Vec::new(),
                insts: vec![
                    Inst::Const { dst: ValueId(0), value: MirConst::Int(2) },
                    Inst::Const { dst: ValueId(1), value: MirConst::Int(3) },
                    Inst::Const { dst: ValueId(2), value: MirConst::Int(4) },
                    Inst::BinOp { dst: ValueId(3), op: BinOp::IMul, lhs: ValueId(1), rhs: ValueId(2) },
                    Inst::BinOp { dst: ValueId(4), op: BinOp::IAdd, lhs: ValueId(0), rhs: ValueId(3) },
                ],
                term: Terminator::Return { value: Some(ValueId(4)) },
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
        };
        let mut prog = Program {
            functions: vec![main],
            classes: Vec::new(),
            enums: Vec::new(),
            vtables: Vec::new(),
            statics: Vec::new(),
            entry: FuncId(0),
        };
        let stats = run_program(&mut prog);
        assert_eq!(stats.folds_applied, 2);
        let insts = &prog.functions[0].blocks[0].insts;
        // The two BinOp instructions are rewritten in place to Const.
        match &insts[3] {
            Inst::Const { value: MirConst::Int(v), .. } => assert_eq!(*v, 12),
            other => panic!("expected folded const, got {other:?}"),
        }
        match &insts[4] {
            Inst::Const { value: MirConst::Int(v), .. } => assert_eq!(*v, 14),
            other => panic!("expected folded const, got {other:?}"),
        }
    }

    #[test]
    fn folds_unop_bool() {
        let main = Function {
            name: intern("main"),
            display_name: intern("main"),
            params: Box::new([] as [FuncParam; 0]),
            ret: MirTy::Bool,
            value_tys: vec![MirTy::Bool, MirTy::Bool],
            value_spans: vec![None, None],
            blocks: vec![Block {
                params: Vec::new(),
                insts: vec![
                    Inst::Const { dst: ValueId(0), value: MirConst::Bool(true) },
                    Inst::UnOp { dst: ValueId(1), op: UnOp::BoolNot, src: ValueId(0) },
                ],
                term: Terminator::Return { value: Some(ValueId(1)) },
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
        };
        let mut prog = Program {
            functions: vec![main],
            classes: Vec::new(),
            enums: Vec::new(),
            vtables: Vec::new(),
            statics: Vec::new(),
            entry: FuncId(0),
        };
        let stats = run_program(&mut prog);
        assert_eq!(stats.folds_applied, 1);
        match &prog.functions[0].blocks[0].insts[1] {
            Inst::Const { value: MirConst::Bool(v), .. } => assert!(!*v),
            other => panic!("expected folded const bool, got {other:?}"),
        }
    }
}
