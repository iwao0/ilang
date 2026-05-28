//! `impl Lower` extracted into per-concern submodules: each file
//! declares its own `impl Lower { ... }` block so the methods stay
//! organised without forcing one giant impl in mod.rs.

mod bodies;
mod class;
mod enum_fn;
mod extern_c;
