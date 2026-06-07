// `pub use module as <alias>` is rejected — the umbrella contract
// is to re-publish the inner module under its canonical name; an
// alias on the umbrella side would muddle which name downstream
// callers refer to. Per syntax.md §13b "pub use as <alias> /
// pub use ... as _ are rejected".
//
// expect-error: `pub use M` (namespaced) or `pub use M as _ { * }` (flattened) only

pub use lib_helpers as alias
