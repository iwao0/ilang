//! Promote single-def, non-heap mutable locals into SSA.
//!
//! AST→MIR lowering threads every `let x = expr` through a
//! `LocalId` so reassignments (`x = …`) work. For bindings that
//! never get reassigned the local is overkill — every `UseLocal`
//! is just an alias for the one and only `DefLocal`'s source.
//!
//! This pass replaces those `UseLocal` reads with a direct
//! reference to the def's `ValueId`, then drops both the `DefLocal`
//! and the now-orphaned `UseLocal` instructions. Downstream
//! consumers see the def value directly, which:
//!
//! * Lets `const_fold` reach across `let a = 2 * 3; let b = 7 - 4;
//!   a * b` — without promotion, the `a` / `b` references arrive
//!   as `UseLocal` ValueIds the folder can't trace back to a Const.
//! * Lets `inline` consider functions that only used Locals to
//!   bridge a single `let` (one of the eligibility blockers was
//!   any `DefLocal` / `UseLocal` in the candidate body).
//!
//! Heap-typed locals are skipped — the local slot is what carries
//! the ARC +1 share that scope-exit `Release`s; substituting the
//! def value would change whose Release fires.

use std::collections::HashMap;

use crate::inst::{Inst, LocalId, ValueId};
use crate::program::{Function, FunctionKind, Program};

#[derive(Debug, Default, Clone, Copy)]
pub struct Stats {
    pub locals_promoted: usize,
    pub uses_rewritten: usize,
}

impl std::ops::AddAssign for Stats {
    fn add_assign(&mut self, rhs: Self) {
        self.locals_promoted += rhs.locals_promoted;
        self.uses_rewritten += rhs.uses_rewritten;
    }
}

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
    // Count `DefLocal`s per LocalId and remember which block holds
    // the def. Single-def locals are the candidates; we also need
    // the def's block to enforce same-block usage.
    let mut def_count: HashMap<LocalId, u32> = HashMap::new();
    let mut def_value: HashMap<LocalId, ValueId> = HashMap::new();
    let mut def_block: HashMap<LocalId, usize> = HashMap::new();
    for (bi, block) in func.blocks.iter().enumerate() {
        for inst in &block.insts {
            if let Inst::DefLocal { local, value } = inst {
                *def_count.entry(*local).or_insert(0) += 1;
                def_value.entry(*local).or_insert(*value);
                def_block.entry(*local).or_insert(bi);
            }
        }
    }
    // Keep only single-def, non-heap locals whose every UseLocal
    // sits in the same block as the DefLocal. Cross-block
    // promotion would substitute a ValueId defined in block A for
    // a use in block B — if A doesn't dominate B, codegen blows
    // up at `vmap[..]` lookup time. Same-block is a sufficient
    // (conservative) dominance proof and avoids needing a full
    // dominator analysis here.
    let mut promotable: HashMap<LocalId, ValueId> = HashMap::new();
    let mut use_blocks: HashMap<LocalId, Vec<usize>> = HashMap::new();
    for (bi, block) in func.blocks.iter().enumerate() {
        for inst in &block.insts {
            if let Inst::UseLocal { local, .. } = inst {
                use_blocks.entry(*local).or_default().push(bi);
            }
        }
    }
    for (loc, count) in &def_count {
        if *count != 1 {
            continue;
        }
        if let Some(ty) = func.local_tys.get(loc.0 as usize) {
            if ty.is_heap() {
                continue;
            }
        }
        let &db = match def_block.get(loc) {
            Some(b) => b,
            None => continue,
        };
        if let Some(usages) = use_blocks.get(loc) {
            if usages.iter().any(|&ub| ub != db) {
                continue;
            }
        }
        if let Some(&src) = def_value.get(loc) {
            promotable.insert(*loc, src);
        }
    }
    if promotable.is_empty() {
        return Stats::default();
    }
    // First pass: build use→def-value remap by walking UseLocal
    // instructions whose local is in `promotable`. The UseLocal
    // itself becomes a tombstone — we'll filter it out in the
    // second pass alongside the DefLocal.
    let mut value_remap: HashMap<ValueId, ValueId> = HashMap::new();
    for block in &func.blocks {
        for inst in &block.insts {
            if let Inst::UseLocal { dst, local } = inst {
                if let Some(&src) = promotable.get(local) {
                    value_remap.insert(*dst, src);
                }
            }
        }
    }
    // Second pass: apply the remap to every value reference (uses)
    // in every inst + terminator, and filter out the now-dead
    // DefLocal / UseLocal pairs.
    let stats = Stats {
        locals_promoted: promotable.len(),
        uses_rewritten: value_remap.len(),
    };
    for block in func.blocks.iter_mut() {
        let old = std::mem::take(&mut block.insts);
        let mut new = Vec::with_capacity(old.len());
        for mut inst in old {
            // Drop the dead Def/Use pairs for promoted locals.
            match &inst {
                Inst::DefLocal { local, .. } if promotable.contains_key(local) => continue,
                Inst::UseLocal { local, .. } if promotable.contains_key(local) => continue,
                _ => {}
            }
            super::util::remap_inst(&mut inst, |v| {
                if let Some(&n) = value_remap.get(v) {
                    *v = n;
                }
            });
            new.push(inst);
        }
        block.insts = new;
        super::util::remap_terminator(&mut block.term, &value_remap);
    }
    // The locals themselves stay in `local_tys` — removing them
    // would force renumbering every other `LocalId`. The codegen
    // tolerates orphan slots (no DefLocal / UseLocal references)
    // because the Cranelift Variable for an unread slot is just
    // dead.
    stats
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inst::{BinOp, BlockId, Inst, MirConst, Terminator};
    use crate::program::{Block, FuncParam, Function, FunctionKind};
    use crate::types::MirTy;

    fn intern(s: &str) -> ilang_ast::Symbol {
        ilang_ast::Symbol::intern(s)
    }

    /// Lowered shape of `fn f(): i64 { let a = 2 * 3; a + 1 }`.
    /// After promotion the `def_local %0 = v3` and `v5 = use_local
    /// %0` go away, and the final `iadd v5, v6` reads `v3` directly.
    #[test]
    fn promotes_single_def_primitive_local() {
        let blocks = vec![Block {
            params: Vec::new(),
            insts: vec![
                Inst::Const { dst: ValueId(0), value: MirConst::Int(2) },
                Inst::Const { dst: ValueId(1), value: MirConst::Int(3) },
                Inst::BinOp {
                    dst: ValueId(2),
                    op: BinOp::IMul,
                    lhs: ValueId(0),
                    rhs: ValueId(1),
                },
                Inst::DefLocal { local: LocalId(0), value: ValueId(2) },
                Inst::UseLocal { dst: ValueId(3), local: LocalId(0) },
                Inst::Const { dst: ValueId(4), value: MirConst::Int(1) },
                Inst::BinOp {
                    dst: ValueId(5),
                    op: BinOp::IAdd,
                    lhs: ValueId(3),
                    rhs: ValueId(4),
                },
            ],
            term: Terminator::Return { value: Some(ValueId(5)) },
        }];
        let mut func = Function {
            name: intern("f"),
            display_name: intern("f"),
            params: Box::new([] as [FuncParam; 0]),
            ret: MirTy::I64,
            value_tys: vec![MirTy::I64; 6],
            value_spans: vec![None; 6],
            blocks,
            entry: BlockId(0),
            kind: FunctionKind::Local,
            closure_env: None,
            span: None,
            local_tys: vec![MirTy::I64],
            c_symbol: None,
            is_optional: false,
            libs: Vec::new(),
            is_variadic: false,
        };
        let stats = run_function(&mut func);
        assert_eq!(stats.locals_promoted, 1);
        assert_eq!(stats.uses_rewritten, 1);
        let insts = &func.blocks[0].insts;
        // DefLocal + UseLocal stripped; remaining: 2 Const, 1 BinOp,
        // 1 Const, 1 BinOp = 5 insts.
        assert_eq!(insts.len(), 5);
        // Final BinOp now consumes ValueId(2) (the IMul's dst)
        // directly, not the dead UseLocal's ValueId(3).
        if let Inst::BinOp { lhs, rhs, .. } = &insts[4] {
            assert_eq!(*lhs, ValueId(2));
            assert_eq!(*rhs, ValueId(4));
        } else {
            panic!("expected final BinOp, got {:?}", insts[4]);
        }
    }

    #[test]
    fn skips_multi_def_local() {
        // `let x = 1; x = 2` — two DefLocals; promotion must back off.
        let blocks = vec![Block {
            params: Vec::new(),
            insts: vec![
                Inst::Const { dst: ValueId(0), value: MirConst::Int(1) },
                Inst::DefLocal { local: LocalId(0), value: ValueId(0) },
                Inst::Const { dst: ValueId(1), value: MirConst::Int(2) },
                Inst::DefLocal { local: LocalId(0), value: ValueId(1) },
                Inst::UseLocal { dst: ValueId(2), local: LocalId(0) },
            ],
            term: Terminator::Return { value: Some(ValueId(2)) },
        }];
        let mut func = Function {
            name: intern("f"),
            display_name: intern("f"),
            params: Box::new([] as [FuncParam; 0]),
            ret: MirTy::I64,
            value_tys: vec![MirTy::I64; 3],
            value_spans: vec![None; 3],
            blocks,
            entry: BlockId(0),
            kind: FunctionKind::Local,
            closure_env: None,
            span: None,
            local_tys: vec![MirTy::I64],
            c_symbol: None,
            is_optional: false,
            libs: Vec::new(),
            is_variadic: false,
        };
        let stats = run_function(&mut func);
        assert_eq!(stats.locals_promoted, 0);
        // Body must be untouched.
        assert_eq!(func.blocks[0].insts.len(), 5);
    }

    #[test]
    fn skips_heap_local() {
        // `let s: string = "hi"` — heap-typed; ARC ties value to
        // the slot, so we leave it alone.
        let blocks = vec![Block {
            params: Vec::new(),
            insts: vec![
                Inst::Const { dst: ValueId(0), value: MirConst::Str(intern("hi")) },
                Inst::DefLocal { local: LocalId(0), value: ValueId(0) },
                Inst::UseLocal { dst: ValueId(1), local: LocalId(0) },
            ],
            term: Terminator::Return { value: Some(ValueId(1)) },
        }];
        let mut func = Function {
            name: intern("f"),
            display_name: intern("f"),
            params: Box::new([] as [FuncParam; 0]),
            ret: MirTy::Str,
            value_tys: vec![MirTy::Str, MirTy::Str],
            value_spans: vec![None; 2],
            blocks,
            entry: BlockId(0),
            kind: FunctionKind::Local,
            closure_env: None,
            span: None,
            local_tys: vec![MirTy::Str],
            c_symbol: None,
            is_optional: false,
            libs: Vec::new(),
            is_variadic: false,
        };
        let stats = run_function(&mut func);
        assert_eq!(stats.locals_promoted, 0);
    }
}
