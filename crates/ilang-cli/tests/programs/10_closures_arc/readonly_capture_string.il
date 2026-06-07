// Read-only capture is a value snapshot for STRING (heap, ARC) —
// reassigning the outer string after the closure is built doesn't
// bleed into the closure. The captured pointer is retained at
// capture time and lives in the closure env until the closure is
// dropped. Companion to `readonly_capture_snapshot.il` (i64).
// Per syntax.md §6 "every capture type is supported".
use std.test as test

let name = "alice"
let greet = fn(): string { "hi " + name }
test.expectStr(greet(), "hi alice")

// Reassign the outer; the closure still holds its snapshot.
name = "bob"
test.expectStr(greet(), "hi alice")

// Build a second closure after the reassignment — captures the
// new value. Confirms snapshots are per-build, not per-read.
let greet2 = fn(): string { "hi " + name }
test.expectStr(greet2(), "hi bob")

// The first closure is unaffected by the second's snapshot.
test.expectStr(greet(), "hi alice")
