// Sibling module for the use_*.il tests. When run standalone (the
// harness walks every .il), this file just declares fns and a const
// — no assertions, exits cleanly.

pub fn double(n: i64): i64 { n * 2 }
pub fn triple(n: i64): i64 { n * 3 }
pub fn square(n: i64): i64 { n * n }

pub const TEN: i64 = 10
