// Top hop of the 3-level `pub use` chain.
// Re-exports `mid` (which itself re-exports `core`) under its
// canonical name; aliasing `pub use` is rejected by design.
pub use lib_pubchain_mid
