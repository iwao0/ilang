// Interface methods declared outside an `@extern(C) {}` block follow
// the same "no raw C pointers" rule as fn / class signatures. A
// top-level `pub interface` referencing `*const u8` in a parameter
// must surface as a type error at declaration time, not leak
// through to call sites.
//
// expect-error: raw C pointer types are only nameable inside

pub interface Bad {
    foo(p: *const u8): i32
}
