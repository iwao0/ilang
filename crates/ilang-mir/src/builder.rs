//! `FunctionBuilder` — convenience for assembling a MIR `Function`
//! incrementally during AST→MIR lowering.

use crate::inst::{BlockId, Inst, Terminator, ValueId};
use crate::program::{Block, Function, FunctionKind};
use crate::types::MirTy;
use ilang_ast::{Span, Symbol};

pub struct FunctionBuilder {
    pub name: Symbol,
    pub display_name: Symbol,
    pub ret: MirTy,
    pub kind: FunctionKind,
    value_tys: Vec<MirTy>,
    value_spans: Vec<Option<Span>>,
    blocks: Vec<Option<Block>>,
    cur_block: Option<BlockId>,
    entry: Option<BlockId>,
    span: Option<Span>,
    local_tys: Vec<MirTy>,
}

impl FunctionBuilder {
    pub fn new(name: Symbol, display_name: Symbol, ret: MirTy, kind: FunctionKind) -> Self {
        Self {
            name,
            display_name,
            ret,
            kind,
            value_tys: Vec::new(),
            value_spans: Vec::new(),
            blocks: Vec::new(),
            cur_block: None,
            entry: None,
            span: None,
            local_tys: Vec::new(),
        }
    }

    /// Reserve a fresh mutable local slot. Returns the `LocalId`.
    pub fn new_local(&mut self, ty: MirTy) -> crate::inst::LocalId {
        let id = crate::inst::LocalId(self.local_tys.len() as u32);
        self.local_tys.push(ty);
        id
    }

    pub fn set_span(&mut self, span: Span) {
        self.span = Some(span);
    }

    /// Reserve a new SSA value of the given type. Caller must ensure
    /// it is defined exactly once (by an `Inst` or as a `Block::params`
    /// entry).
    pub fn new_value(&mut self, ty: MirTy) -> ValueId {
        self.new_value_with_span(ty, None)
    }

    pub fn new_value_with_span(&mut self, ty: MirTy, span: Option<Span>) -> ValueId {
        let id = ValueId(self.value_tys.len() as u32);
        self.value_tys.push(ty);
        self.value_spans.push(span);
        id
    }

    pub fn ty_of(&self, v: ValueId) -> &MirTy {
        &self.value_tys[v.0 as usize]
    }

    /// Allocate a new (empty, terminator-less) block and return its ID.
    /// The block must be filled in via `seal_block` (or
    /// `set_terminator`) before the function is finalised.
    pub fn new_block(&mut self) -> BlockId {
        let id = BlockId(self.blocks.len() as u32);
        self.blocks.push(Some(Block {
            params: Vec::new(),
            insts: Vec::new(),
            term: Terminator::Unreachable,
        }));
        if self.entry.is_none() {
            self.entry = Some(id);
        }
        id
    }

    /// Add a parameter to the given block, returning the SSA value
    /// reachable inside the block.
    pub fn add_block_param(&mut self, block: BlockId, ty: MirTy) -> ValueId {
        let v = self.new_value(ty);
        self.blocks[block.0 as usize].as_mut().unwrap().params.push(v);
        v
    }

    pub fn switch_to(&mut self, block: BlockId) {
        self.cur_block = Some(block);
    }

    pub fn current_block(&self) -> BlockId {
        self.cur_block.expect("no current block")
    }

    pub fn block(&self, id: BlockId) -> &crate::program::Block {
        self.blocks[id.0 as usize]
            .as_ref()
            .expect("block taken / not yet defined")
    }

    pub fn push_inst(&mut self, inst: Inst) {
        let b = self.cur_block.expect("no current block");
        self.blocks[b.0 as usize]
            .as_mut()
            .expect("current block taken")
            .insts
            .push(inst);
    }

    pub fn set_terminator(&mut self, term: Terminator) {
        let b = self.cur_block.expect("no current block");
        let blk = self.blocks[b.0 as usize].as_mut().expect("current block taken");
        blk.term = term;
    }

    pub fn finish(self, params: Box<[crate::program::FuncParam]>) -> Function {
        let entry = self.entry.expect("function has no entry block");
        let blocks: Vec<Block> = self.blocks.into_iter().map(|b| b.expect("block hole")).collect();
        Function {
            name: self.name,
            display_name: self.display_name,
            params,
            ret: self.ret,
            value_tys: self.value_tys,
            value_spans: self.value_spans,
            blocks,
            entry,
            kind: self.kind,
            closure_env: None,
            span: self.span,
            local_tys: self.local_tys,
            c_symbol: None,
            is_optional: false,
            libs: Vec::new(),
            is_variadic: false,
        }
    }
}
