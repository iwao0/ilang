//! `impl Lower` extracted into per-concern submodules: each file
//! declares its own `impl Lower { ... }` block so the methods stay
//! organised without forcing one giant impl in mod.rs.

mod extern_c;
