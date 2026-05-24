//! Top-level `let` slot storage.
//!
//! Used by both the REPL (cross-chunk persistence) and the AOT entry
//! (in-process persistence of top-level mutable lets referenced by
//! named functions). Lower emits `__repl_store_slot(idx, value)`
//! after each top-level let's initialiser and `__repl_load_slot(idx)`
//! wherever a fn body references the binding.

use std::sync::{Mutex, OnceLock};

fn repl_slot_storage() -> &'static Mutex<Vec<i64>> {
    static SLOTS: OnceLock<Mutex<Vec<i64>>> = OnceLock::new();
    SLOTS.get_or_init(|| Mutex::new(Vec::new()))
}

#[unsafe(export_name = "$repl.loadSlot")]
pub extern "C" fn __repl_load_slot(idx: i64) -> i64 {
    let g = repl_slot_storage().lock().expect("repl slots poisoned");
    g.get(idx as usize).copied().unwrap_or(0)
}

#[unsafe(export_name = "$repl.storeSlot")]
pub extern "C" fn __repl_store_slot(idx: i64, value: i64) {
    let mut g = repl_slot_storage().lock().expect("repl slots poisoned");
    let need = (idx as usize) + 1;
    if g.len() < need {
        g.resize(need, 0);
    }
    g[idx as usize] = value;
}

/// Public reset hook so REPL sessions starting fresh don't carry
/// over slots from a previous in-process run.
pub fn reset_repl_slots() {
    let mut g = repl_slot_storage().lock().expect("repl slots poisoned");
    g.clear();
}
