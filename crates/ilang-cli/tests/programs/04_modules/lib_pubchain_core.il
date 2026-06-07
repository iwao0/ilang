// Leaf of a 3-level `pub use` chain. Reached by callers as
// `top.mid.core.echo3()`. The dotted prefix is registered by
// the parser's `renormalize_merged` so each intermediate
// (`top`, `top.mid`, `top.mid.core`) resolves as a module.
pub fn echo3(): string { "leaf3" }
