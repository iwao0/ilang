//! Constant-condition branch elimination.
//!
//! Looks at every `Terminator::CondBr` / `Terminator::Switch` and
//! rewrites it to an unconditional `Br` when the condition / scrutinee
//! is a previously-defined `Inst::Const`. Pairs naturally with
//! `const_fold`: a comparison like `1 < 2` folds to `Bool(true)`,
//! then this pass turns the surrounding `if 1 < 2 { … }` into a
//! straight-line jump.
//!
//! Doesn't currently delete the abandoned target block — its
//! instructions become unreachable but stay in `func.blocks`. A
//! follow-up reachability pass can sweep them.

use std::collections::HashMap;

use crate::inst::{Inst, MirConst, Terminator, ValueId};
use crate::program::{Function, FunctionKind, Program};

#[derive(Debug, Default, Clone, Copy)]
pub struct Stats {
    pub branches_folded: usize,
}

impl std::ops::AddAssign for Stats {
    fn add_assign(&mut self, rhs: Self) {
        self.branches_folded += rhs.branches_folded;
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
    // Build a function-wide ValueId → MirConst table by walking
    // every Const instruction. SSA gives us one def per ValueId.
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
        if let Some(new_term) = try_fold_terminator(&block.term, &table) {
            block.term = new_term;
            stats.branches_folded += 1;
        }
    }
    stats
}

fn try_fold_terminator(
    term: &Terminator,
    table: &HashMap<ValueId, MirConst>,
) -> Option<Terminator> {
    match term {
        Terminator::CondBr {
            cond,
            then_block,
            then_args,
            else_block,
            else_args,
        } => {
            let b = match table.get(cond)? {
                MirConst::Bool(b) => *b,
                _ => return None,
            };
            let (dst, args) = if b {
                (*then_block, then_args.clone())
            } else {
                (*else_block, else_args.clone())
            };
            Some(Terminator::Br { dst, args })
        }
        Terminator::Switch {
            scrutinee,
            cases,
            default,
            default_args,
        } => {
            let n = match table.get(scrutinee)? {
                MirConst::Int(v) => *v,
                _ => return None,
            };
            for case in cases.iter() {
                if case.value == n {
                    return Some(Terminator::Br {
                        dst: case.dst,
                        args: case.args.clone(),
                    });
                }
            }
            Some(Terminator::Br {
                dst: *default,
                args: default_args.clone(),
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inst::{BlockId, Inst, MirConst, Terminator};
    use crate::program::{Block, FuncParam, Function, FunctionKind};
    use crate::types::MirTy;

    fn intern(s: &str) -> ilang_ast::Symbol {
        ilang_ast::Symbol::intern(s)
    }

    #[test]
    fn folds_condbr_on_known_true() {
        // Block 0: v0 = const true; CondBr v0, then=1, else=2
        // Block 1: return const 1
        // Block 2: return const 2
        let blocks = vec![
            Block {
                params: Vec::new(),
                insts: vec![Inst::Const {
                    dst: ValueId(0),
                    value: MirConst::Bool(true),
                }],
                term: Terminator::CondBr {
                    cond: ValueId(0),
                    then_block: BlockId(1),
                    then_args: Box::new([]),
                    else_block: BlockId(2),
                    else_args: Box::new([]),
                },
            },
            Block {
                params: Vec::new(),
                insts: vec![Inst::Const {
                    dst: ValueId(1),
                    value: MirConst::Int(1),
                }],
                term: Terminator::Return { value: Some(ValueId(1)) },
            },
            Block {
                params: Vec::new(),
                insts: vec![Inst::Const {
                    dst: ValueId(2),
                    value: MirConst::Int(2),
                }],
                term: Terminator::Return { value: Some(ValueId(2)) },
            },
        ];
        let mut func = Function {
            name: intern("f"),
            display_name: intern("f"),
            params: Box::new([] as [FuncParam; 0]),
            ret: MirTy::I64,
            value_tys: vec![MirTy::Bool, MirTy::I64, MirTy::I64],
            value_spans: vec![None; 3],
            blocks,
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
        let stats = run_function(&mut func);
        assert_eq!(stats.branches_folded, 1);
        match &func.blocks[0].term {
            Terminator::Br { dst, .. } => assert_eq!(*dst, BlockId(1)),
            other => panic!("expected Br to block 1, got {other:?}"),
        }
    }
}
