//! Local environment + loop frame for the lowerer.
//!
//! [`Env`] is the lexical scope stack — a `Vec<Vec<(name, Binding)>>`
//! pushed/popped at every block / loop body. A [`Binding`] is one of:
//!
//! - `Ssa`: an immutable `let` carrying its SSA `ValueId` directly.
//! - `Local`: a mutable binding backed by a Cranelift `LocalId`.
//!   Reads emit `UseLocal`; writes emit `DefLocal`.
//! - `Cell`: a heap-cell-backed binding (a 1-element array used as a
//!   shared box). Reserved for the cell-capture pattern.
//!
//! [`LoopFrame`] is the per-loop entry on `BodyCx::loops` — it
//! records the env depth at loop entry and the continue/break block
//! targets the terminator lowering needs.

use ilang_ast::Symbol;

use crate::inst::{BlockId, LocalId, ValueId};
use crate::types::MirTy;

#[derive(Clone)]
pub(super) enum Binding {
    /// Immutable let — directly carries the SSA value.
    Ssa(ValueId, MirTy),
    /// Mutable local — backed by a `LocalId` slot. Reads emit
    /// `UseLocal`; writes emit `DefLocal`.
    Local(LocalId, MirTy),
    /// Heap-cell-backed binding — a 1-element array used as a shared
    /// box between an outer scope and any closures that capture +
    /// mutate this name. Reads / writes go through `ArrayLoad` /
    /// `ArrayStore` at index 0.
    ///
    /// As of the per-closure cell capture refactor (commit 727a814)
    /// no construction site remains: each writing closure now
    /// allocates its own private cell at the construction call. The
    /// variant + match arms are kept so the lookup_var / assign_var
    /// helpers stay defensive against any future re-introduction.
    #[allow(dead_code)]
    Cell(ValueId, MirTy),
}

#[derive(Default)]
pub(super) struct Env {
    pub(super) scopes: Vec<Vec<(Symbol, Binding)>>,
}

impl Env {
    pub(super) fn enter_scope(&mut self) {
        self.scopes.push(Vec::new());
    }
    pub(super) fn exit_scope(&mut self) {
        self.scopes.pop();
    }
    pub(super) fn bind(&mut self, name: Symbol, v: ValueId, ty: MirTy) {
        if self.scopes.is_empty() {
            self.scopes.push(Vec::new());
        }
        self.scopes
            .last_mut()
            .unwrap()
            .push((name, Binding::Ssa(v, ty)));
    }
    #[allow(dead_code)]
    pub(super) fn bind_cell(&mut self, name: Symbol, cell_v: ValueId, ty: MirTy) {
        if self.scopes.is_empty() {
            self.scopes.push(Vec::new());
        }
        self.scopes
            .last_mut()
            .unwrap()
            .push((name, Binding::Cell(cell_v, ty)));
    }
    pub(super) fn bind_local(&mut self, name: Symbol, lid: LocalId, ty: MirTy) {
        if self.scopes.is_empty() {
            self.scopes.push(Vec::new());
        }
        self.scopes
            .last_mut()
            .unwrap()
            .push((name, Binding::Local(lid, ty)));
    }
    /// Returns true if the binding existed (a fresh value was placed).
    /// For immutable bindings the value replaces the slot's payload;
    /// mutable bindings stay as Local — the caller is responsible for
    /// emitting a `DefLocal`.
    pub(super) fn rebind(&mut self, name: Symbol, v: ValueId, ty: MirTy) -> bool {
        for scope in self.scopes.iter_mut().rev() {
            for entry in scope.iter_mut().rev() {
                if entry.0 == name {
                    if matches!(entry.1, Binding::Local(..) | Binding::Cell(..)) {
                        // Caller emits DefLocal / ArrayStore — binding
                        // shape stays.
                        return true;
                    }
                    *entry = (name, Binding::Ssa(v, ty));
                    return true;
                }
            }
        }
        false
    }
    /// Convenience: a `lookup` that emits a `UseLocal` for mutable
    /// bindings. Returns the (ValueId, MirTy) ready for use as an
    /// expression value. For locals, the caller passes a closure that
    /// allocates a fresh ValueId and pushes the UseLocal inst.
    pub(super) fn lookup_binding(&self, name: Symbol) -> Option<Binding> {
        // Scopes are typically tiny (single-digit bindings each) so
        // linear scan with Symbol == comparison (interned ids) beats
        // HashMap hashing+probing. The reverse iteration honors the
        // shadowing rule: inner scopes / later bindings win.
        for scope in self.scopes.iter().rev() {
            for (n, b) in scope.iter().rev() {
                if *n == name {
                    return Some(b.clone());
                }
            }
        }
        None
    }
}

pub(super) struct LoopFrame {
    /// `env.scopes.len()` recorded right when the loop body's
    /// outer scope was pushed. A `break` from anywhere inside the
    /// body needs to release every heap-typed binding introduced
    /// in scopes pushed since this point — `lower_block`'s
    /// scope-exit release pass is bypassed by an early jump.
    pub(super) env_depth_at_entry: usize,
    /// Block to jump to on `continue`.
    pub(super) continue_target: BlockId,
    /// Block to jump to on `break`. The block has zero block params
    /// for `while`/`for`/value-less `break`; a `loop` gains a param
    /// the first time a `break v` appears (lazy attach).
    pub(super) break_target: BlockId,
}
