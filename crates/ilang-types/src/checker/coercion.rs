//! Shared subtype-aware coercion rules between assignment checks
//! (`literal_assignable_with`) and overload-resolution scoring
//! (`score_arg`). Centralising these here keeps the rule table in
//! one place — every site that previously inlined the
//! `Object → Object`, `Object → Weak`, or related shape needed the
//! same pair of conditions, and they kept drifting apart whenever
//! one site added a new wrinkle.

use ilang_ast::{Symbol, Type};

/// Score the conversion `Object(from_cid)` → `to`, treating the
/// outcome's cost the same way `score_arg` reports it: same-class
/// or subclass into an Object slot scores `5 + depth`; same-class
/// or subclass into a Weak slot scores `4 + depth`. Returns `None`
/// when the receiver's shape isn't an Object / Weak slot or the
/// classes aren't related.
///
/// `is_sub(child, ancestor)` returns the inheritance / interface-
/// implementation distance (0 for the same class, `n` for an
/// `n`-step parent chain or any matching interface) when the
/// relation holds.
pub(super) fn class_pair_coercion<F>(
    from_cid: Symbol,
    to: &Type,
    is_sub: &F,
) -> Option<u32>
where
    F: Fn(Symbol, Symbol) -> Option<u32>,
{
    match to {
        Type::Object(p) => is_sub(from_cid, *p).map(|d| 5 + d),
        Type::Weak(inner) => {
            if let Type::Object(p) = inner.as_ref() {
                // Same-class is the cheapest weak edge (cost 4);
                // subclass adds the inheritance distance.
                if from_cid == *p {
                    return Some(4);
                }
                is_sub(from_cid, *p).map(|d| 4 + d)
            } else {
                None
            }
        }
        _ => None,
    }
}
