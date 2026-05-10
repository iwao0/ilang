// Sibling module — exercises a sub-module's own top-level stmt
// reading a non-pub static field. The cross-module visibility
// check must permit this because the access happens within the
// declaring module.

pub class Box {
    const cap: i64 = 100
}

let cached = Box.cap
